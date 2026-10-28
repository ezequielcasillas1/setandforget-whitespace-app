import { useEffect, useState } from 'react'
import { PATTERNS } from '../breathing/patterns'
import { DEFAULT_SETTINGS, loadSettings, normalize, saveSettings, type Settings } from '../settings'
import { invoke, isTauri, listen } from '../tauri'
import { validateHotkey } from '../hotkey'
import { DEFAULT_ADD_KIND, DEFAULT_WATCH, addEntry, sameAsDefaults, type WatchKind } from '../watch'

type HookStatus = { id: string; found: boolean; installed: boolean; trustPending: boolean }
// Card 03 build order. Notes are the UX packet's proposed wording (slice 03b, B1).
const AI_TOOLS: { id: string; name: string; note?: string }[] = [
  { id: 'claude', name: 'Claude Code' },
  { id: 'codex', name: 'OpenAI Codex' },
  { id: 'copilot', name: 'GitHub Copilot', note: 'VS Code support is a preview.' },
  { id: 'cursor', name: 'Cursor', note: "Clears when the run finishes. It can't tell when it's waiting for your approval." },
  { id: 'windsurf', name: 'Windsurf', note: "Clears when the run finishes. It can't tell when it's waiting for your approval." },
]

export function hookStatusText(name: string, h: HookStatus | undefined, failed: boolean): string {
  if (failed) return "Couldn't install. Your file wasn't changed."
  if (!h) return ''
  if (!h.found) return `Can't find ${name} on this PC`
  if (!h.installed) return 'Hook not installed'
  return h.id === 'codex' && h.trustPending ? 'Hook installed, waiting for trust in Codex' : 'Hook installed'
}

// Slice 03c status wording (UX packet, proposed).
export const CLOUD_STATUS: Record<string, string> = {
  'no-key': 'No key yet',
  watching: 'Watching your cloud agents',
  working: 'A cloud agent is working',
  rejected: 'Key rejected. Check it in your Cursor Dashboard.',
  unreachable: "Can't reach Cursor. Trying again.",
}
type CloudStatus = { present: boolean; status: string }

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
  const [hooks, setHooks] = useState<HookStatus[]>([])
  const [hookFailed, setHookFailed] = useState<string | null>(null)
  useEffect(() => { if (isTauri()) void invoke<HookStatus[]>('hook_status').then(setHooks).catch(() => {}) }, [])
  // Acts right away, no Save step (UX B1). A failure leaves the button as it was (B2).
  const [cloud, setCloud] = useState<CloudStatus>({ present: false, status: 'no-key' })
  const [keyDraft, setKeyDraft] = useState('')
  const [keyErr, setKeyErr] = useState<string | null>(null)
  const cloudOn = s.adapters.cursorCloud === true
  useEffect(() => {
    if (!isTauri() || !cloudOn) return
    const get = () => void invoke<CloudStatus>('cursor_key_status').then(setCloud).catch(() => {})
    get(); const t = setInterval(get, 3000)
    return () => clearInterval(t)
  }, [cloudOn])
  const saveKey = async () => {
    try { setCloud(await invoke<CloudStatus>('set_cursor_key', { key: keyDraft })); setKeyErr(null) }
    catch (e) { setKeyErr(String(e)) }
    finally { setKeyDraft('') }
  }
  const removeKey = async () => {
    try { setCloud(await invoke<CloudStatus>('clear_cursor_key')); setKeyErr(null) } catch (e) { setKeyErr(String(e)) }
  }
  const cloudRow = (
      <div className="ai-tool">
        <div className="row">
          <label className="ai-name"><input type="checkbox" checked={cloudOn} onChange={(e) => set('adapters', { ...s.adapters, cursorCloud: e.target.checked })} /> Cursor cloud agents</label>
        </div>
        {cloudOn && (cloud.present ? (
          <div className="row"><span>Key saved</span><button onClick={() => void removeKey()}>Remove key</button></div>
        ) : (
          <div className="row">
            <input type="password" autoComplete="off" spellCheck={false} aria-label="Cursor API key" value={keyDraft} onChange={(e) => setKeyDraft(e.target.value)} />
            <button disabled={!keyDraft.trim()} onClick={() => void saveKey()}>Save key</button>
          </div>
        ))}
        {cloudOn && !cloud.present && <p className="helper">Create a key in your Cursor Dashboard. It stays on this PC.</p>}
        {cloudOn && <p className={keyErr || ['rejected', 'unreachable'].includes(cloud.status) ? 'msg' : 'helper'}>{keyErr ?? CLOUD_STATUS[cloud.status] ?? ''}</p>}
        {cloudOn && <p className="helper">Click Save at the bottom to turn checking on or off.</p>}
      </div>
  )
  const toggleHook = async (id: string, installed: boolean) => {
    try { setHooks(await invoke<HookStatus[]>(installed ? 'remove_hook' : 'install_hook', { id })); setHookFailed(null) }
    catch { setHookFailed(id) }
  }
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
  const setDisplay = <K extends keyof Settings['display']>(k: K, v: Settings['display'][K]) => set('display', { ...s.display, [k]: v })
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

      <h3>AI builders</h3>
      {AI_TOOLS.map((t) => {
        const h = hooks.find((x) => x.id === t.id)
        const failed = hookFailed === t.id
        return (
          <div key={t.id} className="ai-tool">
            <div className="row">
              <label className="ai-name"><input type="checkbox" checked={s.adapters[t.id] ?? true} onChange={(e) => set('adapters', { ...s.adapters, [t.id]: e.target.checked })} /> {t.name}</label>
              <button disabled={!h || !h.found} onClick={() => void toggleHook(t.id, !!h?.installed)}>{h?.installed ? 'Remove hook' : 'Install hook'}</button>
            </div>
            {t.note && <p className="helper">{t.note}</p>}
            <p className={failed ? 'msg' : 'helper'}>{hookStatusText(t.name, h, failed)}</p>
            {t.id === 'codex' && h?.installed && h.trustPending && <p className="helper">One more step: in Codex, open /hooks and trust Whitespace's hook.</p>}
            {t.id === 'cursor' && cloudRow}
          </div>
        )
      })}
      <div className="ai-tool">
        <div className="row">
          <label className="ai-name"><input type="checkbox" checked={s.adapters.grokbot ?? false} onChange={(e) => set('adapters', { ...s.adapters, grokbot: e.target.checked })} /> Grok Bot</label>
          <button disabled>Install hook</button>
        </div>
        <p className="helper">Not available yet.</p>
      </div>

      <h2>Look</h2>
      <h3>Show on the whiteout</h3>
      <label className="row">Show verses<input type="checkbox" checked={s.display.verses} onChange={(e) => setDisplay('verses', e.target.checked)} /></label>
      <label className="row">Show breathing<input type="checkbox" checked={s.display.breathing} onChange={(e) => setDisplay('breathing', e.target.checked)} /></label>
      {noVerses && noBreathing && <p className="hint">The whiteout will be a plain white screen.</p>}
      <h3>Screen mode</h3>
      {([['full', 'Full screen'], ['fit', 'Fit to screen (keep taskbar visible)']] as const).map(([id, label]) => (
        <div key={id} role="radio" aria-checked={s.display.screenMode === id} className={`pattern-option${s.display.screenMode === id ? ' active' : ''}`} onClick={() => setDisplay('screenMode', id)}>{label}</div>
      ))}
      <p className="hint">Takes effect the next time the whiteout appears.</p>
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
        {isTauri() && <button onClick={() => void invoke('quit_app').catch(() => setMsg("Couldn't quit. Right-click the tray icon and pick Quit."))}>Quit Whitespace</button>}
        <button className="primary" onClick={save}>Save</button>
        {isTauri() && <button onClick={() => void invoke('show_whiteout_test')}>Show whiteout now (test)</button>}
        {!isTauri() && <button onClick={() => { location.hash = 'whiteout-preview'; location.reload() }}>Preview whiteout</button>}
      </div>
      {isTauri() && <p className="helper">Quit closes Whitespace completely. Unsaved changes aren't kept.</p>}
    </div>
  )
}
