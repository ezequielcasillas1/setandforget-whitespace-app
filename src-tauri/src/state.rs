//! Whiteout state machine (card §2): idle → working → (3 s) → shown → {needs-you | done | panic | exit | cap | quiet} → cleared.
//! Pure and clock-free (times are ms since an arbitrary origin) so it is unit-testable.

use crate::adapters::ToolState;

pub const SHOW_DELAY_MS: u64 = 3_000;
pub const CAP_MS: u64 = 20 * 60 * 1_000;
pub const QUIET_MS: u64 = 90 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearReason { NeedsYou, Done, Panic, ExitButton, Cap, AdapterQuiet, Disabled }

impl ClearReason {
    /// The soft chime plays when the tool needs you or is done (spec).
    pub fn chime(self) -> bool { matches!(self, ClearReason::NeedsYou | ClearReason::Done) }
    /// Needs-you (failed build, Cursor accept prompt) clears with no fade (UX S2.3).
    pub fn instant(self) -> bool { matches!(self, ClearReason::NeedsYou) }
    pub fn as_str(self) -> &'static str {
        match self {
            ClearReason::NeedsYou => "needs-you", ClearReason::Done => "done", ClearReason::Panic => "panic",
            ClearReason::ExitButton => "exit-button", ClearReason::Cap => "cap", ClearReason::AdapterQuiet => "adapter-quiet",
            ClearReason::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase { Idle, Pending { since: u64 }, Shown { since: u64 } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action { None, Show, Clear(ClearReason) }

#[derive(Debug, Clone)]
pub struct Machine { pub phase: Phase, last_signal: u64 }

impl Default for Machine { fn default() -> Self { Self { phase: Phase::Idle, last_signal: 0 } } }

impl Machine {
    /// A state report from an adapter.
    pub fn on_tool(&mut self, s: ToolState, now: u64) -> Action {
        self.last_signal = now;
        match (s, self.phase) {
            (ToolState::Working, Phase::Idle) => { self.phase = Phase::Pending { since: now }; Action::None }
            (ToolState::Working, _) => Action::None,
            (ToolState::NeedsYou, Phase::Shown { .. }) => self.clear(ClearReason::NeedsYou),
            (ToolState::Done, Phase::Shown { .. }) => self.clear(ClearReason::Done),
            (ToolState::NeedsYou | ToolState::Done | ToolState::Idle, Phase::Pending { .. }) => { self.phase = Phase::Idle; Action::None }
            (ToolState::Idle, Phase::Shown { .. }) => self.clear(ClearReason::Done),
            _ => Action::None,
        }
    }
    /// Adapter heartbeat with no state change (still counts as "not quiet").
    pub fn on_signal(&mut self, now: u64) { self.last_signal = now; }

    pub fn tick(&mut self, now: u64) -> Action {
        match self.phase {
            Phase::Pending { since } if now.saturating_sub(since) >= SHOW_DELAY_MS => {
                self.phase = Phase::Shown { since: now };
                Action::Show
            }
            Phase::Pending { .. } if now.saturating_sub(self.last_signal) >= QUIET_MS => { self.phase = Phase::Idle; Action::None }
            Phase::Shown { since } if now.saturating_sub(since) >= CAP_MS => self.clear(ClearReason::Cap),
            Phase::Shown { .. } if now.saturating_sub(self.last_signal) >= QUIET_MS => self.clear(ClearReason::AdapterQuiet),
            _ => Action::None,
        }
    }
    /// Tray "Show whiteout now (test)": skips the delay. Cap and quiet timers still apply.
    pub fn show_now(&mut self, now: u64) -> Action {
        if matches!(self.phase, Phase::Shown { .. }) { return Action::None; }
        self.last_signal = now;
        self.phase = Phase::Shown { since: now };
        Action::Show
    }
    pub fn panic(&mut self) -> Action { self.user_clear(ClearReason::Panic) }
    pub fn exit_button(&mut self) -> Action { self.user_clear(ClearReason::ExitButton) }
    pub fn disable(&mut self) -> Action { self.user_clear(ClearReason::Disabled) }

    fn user_clear(&mut self, r: ClearReason) -> Action {
        match self.phase {
            Phase::Shown { .. } => self.clear(r),
            Phase::Pending { .. } => { self.phase = Phase::Idle; Action::None }
            Phase::Idle => Action::None,
        }
    }
    fn clear(&mut self, r: ClearReason) -> Action { self.phase = Phase::Idle; Action::Clear(r) }
    pub fn is_shown(&self) -> bool { matches!(self.phase, Phase::Shown { .. }) }
}


/// Several tools → one overlay (card §15.2). Working if any enabled adapter is working; the single arm
/// timer lives in `Machine` and isn't reset by a second tool. Clears when all are idle/done. A NeedsYou
/// from any adapter clears right away and doesn't re-arm until a new Working edge.
#[derive(Debug, Default, Clone)]
pub struct Aggregator {
    last: std::collections::HashMap<&'static str, ToolState>,
    latched: bool,
}

impl Aggregator {
    /// One adapter's report for this poll.
    pub fn report(&mut self, m: &mut Machine, id: &'static str, s: ToolState, now: u64) -> Action {
        let prev = self.last.insert(id, s);
        match s {
            ToolState::Working => {
                let edge = prev != Some(ToolState::Working);
                if self.latched && !edge { m.on_signal(now); return Action::None; }
                self.latched = false;
                m.on_tool(ToolState::Working, now)
            }
            ToolState::NeedsYou => { self.latched = true; m.on_tool(ToolState::NeedsYou, now) }
            ToolState::Done | ToolState::Idle => {
                if self.any_working() { m.on_signal(now); Action::None } else { m.on_tool(s, now) }
            }
        }
    }
    /// Adapter disabled or not reporting: forget it so it can't hold the overlay up.
    pub fn forget(&mut self, m: &mut Machine, id: &'static str, now: u64) -> Action {
        if self.last.remove(id) == Some(ToolState::Working) && !self.any_working() && !self.latched { return m.on_tool(ToolState::Done, now); }
        Action::None
    }
    pub fn any_working(&self) -> bool { self.last.values().any(|s| *s == ToolState::Working) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ToolState::*;

    fn working_then_shown() -> Machine {
        let mut m = Machine::default();
        assert_eq!(m.on_tool(Working, 0), Action::None);
        assert_eq!(m.tick(2_999), Action::None);
        assert_eq!(m.tick(3_000), Action::Show);
        m
    }

    #[test] fn shows_after_3s_of_working() { assert!(working_then_shown().is_shown()); }

    #[test] fn needs_you_and_done_clear_immediately_with_chime() {
        let mut m = working_then_shown();
        assert_eq!(m.on_tool(NeedsYou, 4_000), Action::Clear(ClearReason::NeedsYou));
        assert!(ClearReason::NeedsYou.chime());
        let mut m = working_then_shown();
        assert_eq!(m.on_tool(Done, 4_000), Action::Clear(ClearReason::Done));
        assert!(ClearReason::Done.chime());
    }

    #[test] fn short_runs_never_show() {
        let mut m = Machine::default();
        m.on_tool(Working, 0);
        assert_eq!(m.on_tool(Done, 1_500), Action::None);
        assert_eq!(m.tick(10_000), Action::None);
        assert_eq!(m.phase, Phase::Idle);
    }

    #[test] fn cap_at_20_minutes() {
        let mut m = working_then_shown();
        let mut t = 3_000;
        while t < 3_000 + CAP_MS - 1 { t += 30_000; m.on_signal(t); if t < 3_000 + CAP_MS { assert_eq!(m.tick(t.min(3_000 + CAP_MS - 1)), Action::None); } }
        assert_eq!(m.tick(3_000 + CAP_MS), Action::Clear(ClearReason::Cap));
        assert!(!ClearReason::Cap.chime());
    }

    #[test] fn quiet_adapter_clears_after_90s() {
        let mut m = working_then_shown();
        m.on_signal(10_000);
        assert_eq!(m.tick(10_000 + QUIET_MS - 1), Action::None);
        assert_eq!(m.tick(10_000 + QUIET_MS), Action::Clear(ClearReason::AdapterQuiet));
    }

    #[test] fn panic_and_exit_clear_without_chime() {
        let mut m = working_then_shown();
        assert_eq!(m.panic(), Action::Clear(ClearReason::Panic));
        assert_eq!(m.panic(), Action::None);
        let mut m = working_then_shown();
        assert_eq!(m.exit_button(), Action::Clear(ClearReason::ExitButton));
        assert!(!ClearReason::Panic.chime() && !ClearReason::ExitButton.chime());
    }

    #[test] fn test_show_skips_delay_and_still_caps() {
        let mut m = Machine::default();
        assert_eq!(m.show_now(0), Action::Show);
        assert_eq!(m.show_now(5), Action::None);
        assert_eq!(m.tick(QUIET_MS), Action::Clear(ClearReason::AdapterQuiet));
    }

    #[test] fn two_tools_one_arm_timer() {
        let (mut m, mut g) = (Machine::default(), Aggregator::default());
        g.report(&mut m, "cursor", Working, 0);
        g.report(&mut m, "build", Working, 2_000);
        assert_eq!(m.tick(3_000), Action::Show);
    }
    #[test] fn clears_only_when_all_idle_or_done() {
        let (mut m, mut g) = (Machine::default(), Aggregator::default());
        g.report(&mut m, "cursor", Working, 0); g.report(&mut m, "build", Working, 0);
        m.tick(3_000);
        assert_eq!(g.report(&mut m, "cursor", Done, 4_000), Action::None);
        assert!(m.is_shown());
        assert_eq!(g.report(&mut m, "build", Done, 5_000), Action::Clear(ClearReason::Done));
    }
    #[test] fn needs_you_wins_instantly_and_latches_until_new_edge() {
        let (mut m, mut g) = (Machine::default(), Aggregator::default());
        g.report(&mut m, "cursor", Working, 0); g.report(&mut m, "build", Working, 0);
        m.tick(3_000);
        assert_eq!(g.report(&mut m, "build", NeedsYou, 4_000), Action::Clear(ClearReason::NeedsYou));
        assert!(ClearReason::NeedsYou.instant() && !ClearReason::Done.instant());
        // cursor keeps reporting Working: no re-arm
        for t in [4_500, 8_000, 20_000] { assert_eq!(g.report(&mut m, "cursor", Working, t), Action::None); assert_eq!(m.tick(t), Action::None); }
        assert_eq!(m.phase, Phase::Idle);
        // a new Working edge re-arms
        g.report(&mut m, "build", Working, 21_000);
        assert_eq!(m.tick(24_000), Action::Show);
    }
    #[test] fn ongoing_build_keeps_quiet_timer_fed() {
        let (mut m, mut g) = (Machine::default(), Aggregator::default());
        let mut t = 0;
        while t < 200_000 { g.report(&mut m, "build", Working, t); if t == 3_000 { assert_eq!(m.tick(t), Action::Show); } else { assert_eq!(m.tick(t), Action::None); } t += 500; }
        assert!(m.is_shown());
    }
    #[test] fn forgetting_a_disabled_working_tool_clears() {
        let (mut m, mut g) = (Machine::default(), Aggregator::default());
        g.report(&mut m, "build", Working, 0); m.tick(3_000);
        assert_eq!(g.forget(&mut m, "build", 4_000), Action::Clear(ClearReason::Done));
    }
}
