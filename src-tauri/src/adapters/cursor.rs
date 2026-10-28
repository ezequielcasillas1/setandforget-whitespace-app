//! Cursor adapter — STUB. The event source (which Cursor hooks / log files, their paths, and the
//! event → state mapping) is spike S1 in the card (§4, open question 1) and is not defined in the docs,
//! so nothing is read here yet. `poll()` always returns None. Use the tray's "Show whiteout now (test)"
//! to exercise the whiteout until S1 lands.
use super::{Adapter, ToolState};
use std::time::Instant;

pub struct CursorAdapter { last: Instant }

impl CursorAdapter { pub fn new() -> Self { Self { last: Instant::now() } } }

impl Adapter for CursorAdapter {
    fn id(&self) -> &'static str { "cursor" }
    fn display_name(&self) -> &'static str { "Cursor" }
    fn is_target_running(&self) -> bool { false } // process gate (sysinfo) comes with S1
    fn poll(&mut self) -> Option<ToolState> { None }
    fn last_signal_at(&self) -> Instant { self.last }
}
