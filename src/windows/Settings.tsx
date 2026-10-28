import { useEffect, useState } from 'react'
import { PATTERNS } from '../breathing/patterns'
import { DEFAULT_SETTINGS, loadSettings, normalize, saveSettings, type Settings } from '../settings'
import { invoke, isTauri, listen } from '../tauri'
import { validateHotkey } from '../hotkey'
import { DEFAULT_ADD_KIND, DEFAULT_WATCH, addEntry, sameAsDefaults, type WatchKind } from '../watch'

const timingText = (p: (typeof PATTERNS)[number]) => p.phases.map((x) => x.duration).join('-')

export default function SettingsWindow() {
  const [s, setS] = useState<Settings>(DEFAULT_SETTINGS)
  const [msg, setMsg] = useState<string | null>(null)
  const [hotkeyDraft, setHotkeyDraft] = useState(DEFAULT_SETTINGS.panicHotkey)
  const [hotkeyMsg, setHotkeyMsg] = useState<string | null>(null)
  const [watchDraft, setWatchDraft] = useState('')
  const [watchKind, setWatchKind] = useState<WatchKind>(DEFAULT_ADD_KIND)
  const [watchMsg, setWatchMsg] = useState<string | null>(null)
  const [confirmReset, setConfirmReset] = useState(false)
  const addWatch = () => {
    const r = addEntry(s.watch, watchDraft, watchKind)
    if (r.ok) { set('watch', r.list); setWatchDraft(''); setWatchMsg(null) } else setWatchMsg(r.message)
  }
  const resetWatch = () => { set('watch', DEFAULT_WATCH.map((e) => ({ ...e }))); setConfirmReset(false) }
  useEffect(() => { void loadSettings().then((x) => { setS(x); setHotkeyDraft(x.panicHotkey) }) }, [])
  // Tray checkmarks save straight away; keep this window in sync without touching unsaved edits.
  useEffect(() => {
    const sub = listen<Settings['display']>('settings:display', (d) => setS((p) => ({ ...p, display: d })))
    return () => { void sub.then((u) => u()) }
  }, [])
  const setDisplay = (k: keyof Settings['display'], v: boolean) => set('display', { ...s.display, [k]: v })
  const noBreathing = !s.display.breathing
  const noVerses = !s.display.verses
  // Rejected combos keep the previous one (UX packet §4).
  const commitHotkey = () => {
    const r = validateHotkey(hotkeyDraft)
    if (r.ok) { set('panicHotkey', r.value); setHotkeyDraft(r.value); setHotkeyMsg(null) }
    else { setHotkeyDraft(s.panicHotkey); setHotkeyMsg(r.message) }
  }
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((p) => ({ ...p, [k]: v }))
  const save = async () => {
    // Clamp the verse interval before saving: min ≥ 5 s, max ≥ min (Kit bug 2).
    const clamped = normalize(s)
    setS(clamped)
    const warning = await saveSettings(clamped)
    setMsg(warning ?? 'Saved.')
  }
  return (
    <div className="settings">
      <h1>Whitespace settings</h1>

      <h2>Exercise</h2>
      {PATTERNS.map((p) => (
        <div key={p.id} aria-disabled={noBreathing} className={`pattern-option${s.exercise === p.id ? ' active' : ''}${noBreathing ? ' disabled' : ''}`} onClick={() => { if (!noBreathing) set('exercise', p.id) }}>
          <span className="name">{p.name}</span>
          <span className="timing">{timingText(p)}</span>
          <span className="note">{p.note}</span>
        </div>
      ))}

      <h2>Verse timing</h2>
      <div className="row">First verse<span>As soon as breathing starts</span></div>
      <div className="row">New verse every (seconds, changes on an inhale)
        <span>
          <input type="number" min={5} disabled={noVerses} value={s.verseIntervalSec[0]} onChange={(e) => set('verseIntervalSec', [+e.target.value, s.verseIntervalSec[1]])} />{' – '}
          <input type="number" min={5} disabled={noVerses} value={s.verseIntervalSec[1]} onChange={(e) => set('verseIntervalSec', [s.verseIntervalSec[0], +e.target.value])} />
        </span>
      </div>

      <h2>Apps</h2>
      <label className="row">Cursor<input type="checkbox" checked={s.adapters.cursor ?? true} onChange={(e) => set('adapters', { ...s.adapters, cursor: e.target.checked })} /></label>
      <label className="row">Terminal builds<input type="checkbox" checked={s.adapters.build ?? true} onChange={(e) => set('adapters', { ...s.adapters, build: e.target.checked })} /></label>
      <p className="helper">Whiteout comes on while a watched build or command is running.</p>
      <fieldset className="watch-list" disabled={s.adapters.build === false}>
        {s.watch.length === 0 && <p className="helper">No builds watched. Terminal builds won't trigger the whiteout.</p>}
        {s.watch.map((w, i) => (
          <div key={`${w.kind}:${w.match}`} className="row">
            <span><span className="watch-entry">{w.match}</span> <span className="watch-kind">{w.kind}</span></span>
            <button className="text-button" onClick={() => set('watch', s.watch.filter((_, j) => j !== i))}>Remove</button>
          </div>
        ))}
        <div className="row">
          <input type="text" aria-label="Process name or command" value={watchDraft}
            onChange={(e) => { setWatchDraft(e.target.value); setWatchMsg(null) }}
            onKeyDown={(e) => { if (e.key === 'Enter') addWatch() }} />
          <span>
            <select aria-label="Kind" value={watchKind} onChange={(e) => { setWatchKind(e.target.value as WatchKind); setWatchMsg(null) }}>
              <option value="process">Process</option><option value="command">Command</option>
            </select>{' '}
            <button onClick={addWatch}>Add</button>
          </span>
        </div>
        {watchMsg && <p className="msg">{watchMsg}</p>}
        {confirmReset ? (
          <div className="row">Replace your watch list with the defaults? Your changes will be lost.
            <span><button onClick={resetWatch}>Reset</button>{' '}<button onClick={() => setConfirmReset(false)}>Cancel</button></span>
          </div>
        ) : (
          <button onClick={() => { if (sameAsDefaults(s.watch)) resetWatch(); else setConfirmReset(true) }}>Reset to defaults</button>
        )}
      </fieldset>
      <label className="row">Accessibility fallback (opt-in)<input type="checkbox" checked={s.fallbackAccessibility} onChange={(e) => set('fallbackAccessibility', e.target.checked)} /></label>

      <h2>Look</h2>
      <h3>Show on the whiteout</h3>
      <label className="row">Show verses<input type="checkbox" checked={s.display.verses} onChange={(e) => setDisplay('verses', e.target.checked)} /></label>
      <label className="row">Show breathing<input type="checkbox" checked={s.display.breathing} onChange={(e) => setDisplay('breathing', e.target.checked)} /></label>
      {noVerses && noBreathing && <p className="hint">The whiteout will be a plain white screen.</p>}
      <label className="row">Dust theme<input type="checkbox" disabled={noBreathing} checked={s.dustTheme} onChange={(e) => set('dustTheme', e.target.checked)} /></label>
      <label className="row">Reduced motion
        <select value={s.reducedMotion} onChange={(e) => set('reducedMotion', e.target.value as Settings['reducedMotion'])}>
          <option value="system">System</option><option value="on">On</option><option value="off">Off</option>
        </select>
      </label>
      <label className="row">Monitors
        <select value={s.monitors} onChange={(e) => set('monitors', e.target.value as Settings['monitors'])}>
          <option value="all">All monitors</option><option value="active">Active monitor only</option>
        </select>
      </label>

      <h2>Exit and sound</h2>
      <label className="row">Panic hotkey<input type="text" value={hotkeyDraft} onChange={(e) => setHotkeyDraft(e.target.value)} onBlur={commitHotkey} /></label>
      {hotkeyMsg && <p className="msg">{hotkeyMsg}</p>}
      <label className="row">Chime volume<input type="range" min={0} max={1} step={0.05} value={s.chime.volume} disabled={s.chime.muted} onChange={(e) => set('chime', { ...s.chime, volume: +e.target.value })} /></label>
      <label className="row">Mute chime<input type="checkbox" checked={s.chime.muted} onChange={(e) => set('chime', { ...s.chime, muted: e.target.checked })} /></label>
      {msg && <p className="msg">{msg}</p>}

      <div className="actions">
        <button className="primary" onClick={save}>Save</button>
        {isTauri() && <button onClick={() => void invoke('show_whiteout_test')}>Show whiteout now (test)</button>}
        {!isTauri() && <button onClick={() => { location.hash = 'whiteout-preview'; location.reload() }}>Preview whiteout</button>}
      </div>
    </div>
  )
}
