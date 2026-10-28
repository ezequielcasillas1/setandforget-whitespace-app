import { describe, expect, it } from 'vitest'
import { DEFAULT_HOTKEY, REJECTED_MESSAGE, validateHotkey } from '../src/hotkey'
import { DEFAULT_SETTINGS, normalize } from '../src/settings'

describe('panic hotkey', () => {
  it('default is Ctrl+Alt+Shift+Esc and valid', () => {
    expect(DEFAULT_HOTKEY).toBe('Ctrl+Alt+Shift+Esc')
    expect(validateHotkey('ctrl+alt+shift+escape')).toEqual({ ok: true, value: 'Ctrl+Alt+Shift+Esc' })
  })
  it('rejects Ctrl+Alt+Del and Win+L with the proposed wording', () => {
    for (const c of ['Ctrl+Alt+Del', 'Ctrl+Alt+Shift+Delete', 'Win+L', 'Shift+Win+l'])
      expect(validateHotkey(c)).toEqual({ ok: false, message: REJECTED_MESSAGE })
  })
  it('rejects malformed combos', () => {
    expect(validateHotkey('Esc').ok).toBe(false)
    expect(validateHotkey('Ctrl+Shift').ok).toBe(false)
    expect(validateHotkey('Foo+K').ok).toBe(false)
  })
  it('settings normalise bad saved hotkey and chime', () => {
    const s = normalize({ panicHotkey: 'Win+L', chime: { muted: false, volume: 5 } } as never)
    expect(s.panicHotkey).toBe(DEFAULT_SETTINGS.panicHotkey)
    expect(s.chime.volume).toBe(1)
    expect(s.adapterQuietSec).toBe(90)
  })
})
