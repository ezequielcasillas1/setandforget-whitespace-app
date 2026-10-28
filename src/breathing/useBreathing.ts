// Thin React wrapper around breath-engine.ts. Shape follows AER's useBreathing
// (/workspace/apps/aer-breathe/app/src/hooks/useBreathing.ts, copied Oct 27 2026).
import { useCallback, useEffect, useRef, useState } from 'react'
import { initialState, step, type BreathPattern, type BreathState, type PhaseEvent } from './breath-engine'

export function useBreathing(pattern: BreathPattern, durationMinutes: number) {
  const [state, setState] = useState<BreathState>(initialState)
  const stateRef = useRef(state)
  const breathRef = useRef(0)
  const patternRef = useRef(pattern)
  const durationRef = useRef(durationMinutes * 60)
  const onPhaseChangeRef = useRef<((e: PhaseEvent) => void) | null>(null)

  useEffect(() => { patternRef.current = pattern }, [pattern])
  useEffect(() => { durationRef.current = durationMinutes * 60 }, [durationMinutes])

  const reset = useCallback(() => {
    stateRef.current = { ...initialState }
    breathRef.current = 0
    setState(stateRef.current)
  }, [])

  const start = useCallback(() => {
    const next = { ...initialState, running: true, started: true }
    stateRef.current = next
    breathRef.current = 0
    setState(next)
  }, [])

  useEffect(() => {
    if (!state.running) return
    let frame = 0
    let previousTime = performance.now()
    let lastUiUpdate = 0
    const tick = (now: number) => {
      const r = step(patternRef.current, stateRef.current, (now - previousTime) / 1000, durationRef.current)
      previousTime = now
      stateRef.current = r.state
      breathRef.current = r.value
      r.events.forEach((e) => onPhaseChangeRef.current?.(e))
      if (now - lastUiUpdate > 80 || r.state.complete || r.events.length) {
        setState(r.state)
        lastUiUpdate = now
      }
      if (r.state.running) frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [state.running])

  const setOnPhaseChange = useCallback((cb: (e: PhaseEvent) => void) => {
    onPhaseChangeRef.current = cb
  }, [])

  return { state, breathRef, start, reset, setOnPhaseChange }
}
