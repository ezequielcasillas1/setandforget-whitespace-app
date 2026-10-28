// Panic hotkey parsing/validation (card §3 2b; UX packet §1.3 and §4).
export const DEFAULT_HOTKEY = 'Ctrl+Alt+Shift+Esc'
export const REJECTED_MESSAGE = 'Windows reserves this shortcut. Choose another.'
export const CONFLICT_MESSAGE = 'This shortcut is in use by another app. The hotkey will still exit the whiteout.'

const MODS = ['Ctrl', 'Alt', 'Shift', 'Win'] as const
const ALIASES: Record<string, string> = {
  control: 'Ctrl', ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', win: 'Win', super: 'Win', meta: 'Win', cmd: 'Win',
  esc: 'Esc', escape: 'Esc', del: 'Del', delete: 'Del',
}

export type Combo = { mods: string[]; key: string }

export function parseHotkey(input: string): Combo | null {
  const parts = input.split('+').map((p) => p.trim()).filter(Boolean)
  if (parts.length < 2) return null
  const norm = parts.map((p) => ALIASES[p.toLowerCase()] ?? (p.length === 1 ? p.toUpperCase() : p[0].toUpperCase() + p.slice(1)))
  const key = norm[norm.length - 1]
  const mods = norm.slice(0, -1)
  if ((MODS as readonly string[]).includes(key)) return null
  if (!mods.every((m) => (MODS as readonly string[]).includes(m))) return null
  if (new Set(mods).size !== mods.length) return null
  return { mods: MODS.filter((m) => mods.includes(m)), key }
}

export const formatHotkey = (c: Combo) => [...c.mods, c.key].join('+')

/** Reserved by Windows / invisible to the LL hook: anything with Ctrl+Alt+Del, or Win+L. */
export function isReserved(c: Combo) {
  if (c.key === 'Del' && c.mods.includes('Ctrl') && c.mods.includes('Alt')) return true
  if (c.key === 'L' && c.mods.includes('Win')) return true
  return false
}

export type HotkeyResult = { ok: true; value: string } | { ok: false; message: string }

export function validateHotkey(input: string): HotkeyResult {
  const c = parseHotkey(input)
  if (!c) return { ok: false, message: 'Use modifiers plus one key, e.g. Ctrl+Alt+Shift+Esc.' }
  if (isReserved(c)) return { ok: false, message: REJECTED_MESSAGE }
  return { ok: true, value: formatHotkey(c) }
}
