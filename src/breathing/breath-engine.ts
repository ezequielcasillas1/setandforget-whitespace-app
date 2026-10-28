// Pure engine lifted from /workspace/apps/aer-breathe/app/src/hooks/useBreathing.ts (copied Oct 27 2026).
// Same step model as AER's tick(). Changes allowed by the card (§5, §6):
//  - additive inhale: an inhale that follows an inhale eases from 'from' up to 1 instead of restarting at 0;
//  - phase-change events carry phaseIndex so the verse scheduler can act on the cycle-start inhale only.
export type PhaseKind = 'inhale' | 'hold' | 'exhale'
export type BreathPhase = { kind: PhaseKind; duration: number; label: string }
export type BreathPattern = { id: string; name: string; note: string; phases: BreathPhase[] }

export type BreathState = {
  elapsed: number
  phaseIndex: number
  phaseElapsed: number
  cycle: number
  running: boolean
  started: boolean
  complete: boolean
  /** Breath value at the start of the current phase (used by the additive inhale). */
  from: number
}

export const initialState: BreathState = {
  elapsed: 0, phaseIndex: 0, phaseElapsed: 0, cycle: 1,
  running: false, started: false, complete: false, from: 0,
}

export type PhaseEvent = { kind: PhaseKind; phaseIndex: number; cycle: number }

export const smooth = (t: number) => t * t * (3 - 2 * t)

export const cycleDuration = (pattern: BreathPattern) =>
  pattern.phases.reduce((sum, phase) => sum + phase.duration, 0)

function prevIndex(pattern: BreathPattern, i: number) {
  return (i - 1 + pattern.phases.length) % pattern.phases.length
}

export function getBreathValue(pattern: BreathPattern, state: BreathState) {
  const phase = pattern.phases[state.phaseIndex]
  const p = Math.min(1, state.phaseElapsed / phase.duration)
  const previous = pattern.phases[prevIndex(pattern, state.phaseIndex)]
  if (phase.kind === 'inhale') {
    if (previous.kind === 'inhale' && pattern.phases.length > 1) {
      return state.from + (1 - state.from) * smooth(p)
    }
    return smooth(p)
  }
  if (phase.kind === 'exhale') {
    // AER: 1 - smooth(p). After an additive inhale the value is already ~1, so this is unchanged.
    return 1 - smooth(p)
  }
  return previous.kind === 'inhale' ? 1 : 0
}

/**
 * One tick of AER's step model. `delta` is clamped to 0.1 s as in AER.
 * `durationSec` = session length (overlay passes the 20 min cap).
 */
export function step(
  pattern: BreathPattern,
  state: BreathState,
  rawDelta: number,
  durationSec: number,
): { state: BreathState; events: PhaseEvent[]; value: number } {
  const delta = Math.min(Math.max(rawDelta, 0), 0.1)
  const current = { ...state }
  const events: PhaseEvent[] = []
  current.elapsed += delta
  current.phaseElapsed += delta
  const phases = pattern.phases
  while (current.phaseElapsed >= phases[current.phaseIndex].duration) {
    const ending = { ...current, phaseElapsed: phases[current.phaseIndex].duration }
    const endValue = getBreathValue(pattern, ending)
    current.phaseElapsed -= phases[current.phaseIndex].duration
    current.phaseIndex = (current.phaseIndex + 1) % phases.length
    current.from = endValue
    if (current.phaseIndex === 0) current.cycle += 1
    events.push({ kind: phases[current.phaseIndex].kind, phaseIndex: current.phaseIndex, cycle: current.cycle })
  }
  if (current.elapsed >= durationSec) {
    current.elapsed = durationSec
    current.running = false
    current.complete = true
  }
  return { state: current, events, value: getBreathValue(pattern, current) }
}

/** Run the engine for `seconds` in fixed ticks; handy for tests and previews. */
export function simulate(pattern: BreathPattern, seconds: number, dt = 0.05, durationSec = Infinity) {
  let state: BreathState = { ...initialState, running: true, started: true }
  const events: PhaseEvent[] = []
  let value = 0
  const n = Math.round(seconds / dt)
  for (let i = 0; i < n; i++) {
    const r = step(pattern, state, dt, durationSec)
    state = r.state
    value = r.value
    events.push(...r.events)
  }
  return { state, events, value }
}
