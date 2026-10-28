import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useBreathing } from '../breathing/useBreathing'
import { findPattern } from '../breathing/patterns'
import { playChime } from '../chime'
import { DEFAULT_SETTINGS, loadSettings, prefersReducedMotion, type Settings } from '../settings'
import { invoke, isTauri, listen } from '../tauri'
import DustField from '../theme/DustField'
import { useVerses } from '../verses/useVerses'
import { FALLBACK_VERSE, versionLabel } from '../verses/client'

/** 20-minute cap (spec). Rust enforces it too; this keeps the engine bounded. */
const CAP_MINUTES = 20

type ShowPayload = { seed: number }
type ClearPayload = { reason: string; chime: boolean; instant?: boolean }

export default function Whiteout() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS)
  const [visible, setVisible] = useState(false)
  const [seed, setSeed] = useState(() => Date.now() & 0xffffffff)
  const pattern = findPattern(settings.exercise)
  const { state, breathRef, start, reset, setOnPhaseChange } = useBreathing(pattern, CAP_MINUTES)
  const stateRef = useRef(state)
  stateRef.current = state
  const [minSec, maxSec] = settings.verseIntervalSec
  const timing = useMemo(() => ({ minSec, maxSec }), [minSec, maxSec])
  const verses = useVerses(seed, timing)
  const { start: startVerses, stop: stopVerses } = verses
  const settingsRef = useRef(settings)
  settingsRef.current = settings
  const circleRef = useRef<HTMLDivElement>(null)
  const reduced = prefersReducedMotion(settings)
  const [size, setSize] = useState(() => Math.min(innerWidth, innerHeight))

  useEffect(() => {
    const onResize = () => setSize(Math.min(innerWidth, innerHeight))
    addEventListener('resize', onResize)
    return () => removeEventListener('resize', onResize)
  }, [])

  setOnPhaseChange((e) => verses.onPhase(e, stateRef.current.elapsed))

  const begin = useCallback(async (s: number) => {
    setSettings(await loadSettings())
    setSeed(s)
    setVisible(true)
  }, [])

  // Start breathing + verse 1 together once the seed/settings are applied.
  const startedFor = useRef<number | null>(null)
  useEffect(() => {
    if (!visible || startedFor.current === seed) return
    startedFor.current = seed
    const d = settingsRef.current.display
    // The engine also paces verse changes (on an inhale), so it runs unseen when only verses are on.
    if (d.breathing || d.verses) start()
    if (d.verses) void startVerses() // verses off: no scheduler, no verse API calls
  }, [visible, seed, start, startVerses])

  const [instant, setInstant] = useState(false)
  // instant (needs-you, UX S2.3): no fade and no wait for the chime before clearing.
  const end = useCallback(async (chime: boolean, instantClear = false) => {
    const c = settingsRef.current.chime
    const wantChime = chime && !c.muted && c.volume > 0
    if (instantClear) { if (wantChime) void playChime(c.volume) }
    else if (wantChime) await playChime(c.volume)
    setInstant(instantClear)
    setVisible(false)
    if (!instantClear) await new Promise((r) => setTimeout(r, 420))
    stopVerses()
    reset()
    startedFor.current = null
    if (isTauri()) await invoke('whiteout_faded')
  }, [stopVerses, reset])

  // Subscribe once; handlers go through refs so the listeners are never torn down (Kit bug 1).
  const beginRef = useRef(begin); beginRef.current = begin
  const endRef = useRef(end); endRef.current = end
  useEffect(() => {
    const subs = [
      listen<ShowPayload>('whiteout:show', (p) => void beginRef.current(p.seed)),
      listen<ClearPayload>('whiteout:clear', (p) => void endRef.current(p.chime, !!p.instant)),
    ]
    return () => { subs.forEach((s) => s.then((u) => u())) }
  }, [])
  const previewed = useRef(false)
  useEffect(() => {
    if (isTauri() || previewed.current) return
    previewed.current = true
    void beginRef.current(Date.now() & 0xffffffff) // browser preview: show straight away, once
  }, [])

  // Circle scaling from breathRef each frame (static when reduced motion is on).
  useEffect(() => {
    let frame = 0
    const loop = () => {
      const el = circleRef.current
      if (el) el.style.transform = `scale(${reduced ? 1 : 0.72 + 0.28 * (breathRef.current ?? 0)})`
      frame = requestAnimationFrame(loop)
    }
    frame = requestAnimationFrame(loop)
    return () => cancelAnimationFrame(frame)
  }, [reduced, breathRef])

  const exit = () => {
    if (isTauri()) void invoke('request_clear', { reason: 'exit-button' })
    else void end(false).then(() => begin(Date.now() & 0xffffffff))
  }

  // Desktop circle size (TBD in UX packet): max diameter = 36% of min(width, height), AER's centered min(w,h) rule.
  const diameter = Math.round(size * 0.36)
  const phase = pattern.phases[state.phaseIndex]
  const v = verses.verse
  const showBreathing = settings.display.breathing
  const showVerses = settings.display.verses

  return (
    <div className={`whiteout${visible ? ' visible' : ''}${instant && !visible ? ' instant' : ''}`}>
      {showBreathing && settings.dustTheme && visible && <DustField breathRef={breathRef} reduced={reduced} />}
      <div className="stage">
        {/* A part that's off isn't shown; the other keeps its position (UX slice 04). */}
        <div className="circle-wrap" style={{ width: diameter, height: diameter, visibility: showBreathing ? 'visible' : 'hidden' }}>
          {showBreathing && <div ref={circleRef} className="circle" style={{ width: diameter, height: diameter }} />}
        </div>
        <div className="phase-label">{showBreathing && state.started ? phase.label : '\u00a0'}</div>
        <div className="verse" style={{ opacity: showVerses && v ? 1 : 0 }}>
          {showVerses && v?.text}
          {showVerses && v && <span className="verse-ref">{v.display}</span>}
        </div>
        <div className="version">{showVerses && v ? versionLabel(v) : '\u00a0'}</div>
      </div>
      <div className="exit">
        <button type="button" onClick={exit}>Exit</button>
        <span className="hint">{settings.panicHotkey} to exit</span>
      </div>
      {showVerses && <div className="credits">{v?.source ?? FALLBACK_VERSE.source}</div>}
    </div>
  )
}
