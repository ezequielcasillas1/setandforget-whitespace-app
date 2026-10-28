// Settings shape from card §8 / UX packet §1.3. Stored locally (settings.json in the app data dir via Rust;
// localStorage when running in a plain browser with `npm run dev`).
import { isTauri, invoke } from './tauri'
import { DEFAULT_HOTKEY, validateHotkey } from './hotkey'
import { DEFAULT_WATCH, normalizeWatch, type WatchEntry } from './watch'

export type Settings = {
  exercise: string
  verseIntervalSec: [number, number]
  verseFirstAtStart: true
  adapters: Record<string, boolean>
  /** Terminal builds watch list (card §15.3). `adapters.build` is its on/off. */
  watch: WatchEntry[]
  fallbackAccessibility: boolean
  dustTheme: boolean
  reducedMotion: 'system' | 'on' | 'off'
  monitors: 'all' | 'active'
  panicHotkey: string
  /** Fixed in the MVP (not shown in settings). */
  adapterQuietSec: 90
  chime: { muted: boolean; volume: number }
  /** What the whiteout shows (card §17). Both off = plain white. */
  display: { verses: boolean; breathing: boolean; screenMode: 'full' | 'fit' }
}

export const DEFAULT_SETTINGS: Settings = {
  exercise: 'coherence',
  verseIntervalSec: [45, 60],
  verseFirstAtStart: true,
  adapters: { build: true, cursor: true, claude: true, codex: true, copilot: true, windsurf: true, grokbot: false, cursorCloud: false },
  watch: DEFAULT_WATCH,
  fallbackAccessibility: false,
  dustTheme: false,
  reducedMotion: 'system',
  monitors: 'all',
  panicHotkey: DEFAULT_HOTKEY,
  adapterQuietSec: 90,
  chime: { muted: false, volume: 0.25 },
  display: { verses: true, breathing: true, screenMode: 'full' },
}

export function normalize(raw: Partial<Settings> | null | undefined): Settings {
  const s = { ...DEFAULT_SETTINGS, ...(raw ?? {}) } as Settings
  let [a, b] = Array.isArray(s.verseIntervalSec) ? s.verseIntervalSec : DEFAULT_SETTINGS.verseIntervalSec
  a = Math.max(5, Number(a) || 45); b = Math.max(a, Number(b) || 60)
  s.verseIntervalSec = [a, b]
  const c = (s.chime && typeof s.chime === 'object' ? s.chime : DEFAULT_SETTINGS.chime) as Settings['chime']
  s.chime = { muted: !!c.muted, volume: Math.min(1, Math.max(0, Number(c.volume ?? 0.25))) }
  const hk = validateHotkey(String(s.panicHotkey ?? ''))
  s.panicHotkey = hk.ok ? hk.value : DEFAULT_HOTKEY
  s.adapters = { ...DEFAULT_SETTINGS.adapters, ...(s.adapters && typeof s.adapters === 'object' ? s.adapters : {}) }
  const d = (s.display && typeof s.display === 'object' ? s.display : {}) as Partial<Settings['display']>
  s.display = { verses: d.verses !== false, breathing: d.breathing !== false, screenMode: d.screenMode === 'fit' ? 'fit' : 'full' }
  s.watch = normalizeWatch(raw?.watch)
  s.verseFirstAtStart = true
  s.adapterQuietSec = 90
  return s
}

export async function loadSettings(): Promise<Settings> {
  if (isTauri()) return normalize(await invoke<Partial<Settings>>('get_settings'))
  try { return normalize(JSON.parse(localStorage.getItem('ai-overlay:settings') ?? 'null')) } catch { return DEFAULT_SETTINGS }
}

export async function saveSettings(s: Settings): Promise<string | null> {
  if (isTauri()) return invoke<string | null>('save_settings', { settings: s })
  localStorage.setItem('ai-overlay:settings', JSON.stringify(s))
  return null
}

export function prefersReducedMotion(s: Settings) {
  if (s.reducedMotion === 'on') return true
  if (s.reducedMotion === 'off') return false
  return typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches
}
