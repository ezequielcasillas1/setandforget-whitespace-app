import { describe, expect, it } from 'vitest'
import { DEFAULT_WATCH, addEntry, blockedShell, normalizeWatch, sameAsDefaults } from '../src/watch'
import { normalize } from '../src/settings'

describe('watch list', () => {
  it('rejects empty entries', () => {
    expect(addEntry([], '   ', 'command')).toEqual({ ok: false, message: 'Enter a process name or command.' })
  })
  it('blocks duplicates (trimmed, case-insensitive, same kind)', () => {
    const r = addEntry(DEFAULT_WATCH, '  Cargo Build ', 'command')
    expect(r).toEqual({ ok: false, message: "That's already on the list." })
    expect(addEntry(DEFAULT_WATCH, 'tsc.exe', 'command').ok).toBe(true)
  })
  it('blocks shells as process, with or without .exe', () => {
    for (const s of ['cmd.exe', 'powershell.exe', 'pwsh', 'BASH.EXE', 'node.exe']) {
      const r = addEntry([], s, 'process')
      expect(r.ok).toBe(false)
      if (!r.ok) expect(r.message).toContain(`Shells like ${s} can't be watched`)
    }
    expect(blockedShell({ match: 'node.exe', kind: 'command' })).toBe(false)
  })
  it('adds a trimmed entry', () => {
    const r = addEntry([], ' make ', 'command')
    expect(r).toEqual({ ok: true, list: [{ match: 'make', kind: 'command' }] })
  })
  it('silently dedupes and cleans a loaded list', () => {
    expect(normalizeWatch([{ match: 'make', kind: 'command' }, { match: 'MAKE ', kind: 'command' }, { match: 'pwsh.exe', kind: 'process' }, { match: '', kind: 'command' }, 5, { match: 'x', kind: 'bad' }]))
      .toEqual([{ match: 'make', kind: 'command' }])
    expect(normalizeWatch(undefined)).toEqual(DEFAULT_WATCH)
    expect(normalizeWatch([])).toEqual([])
  })
  it('knows when the list matches defaults', () => {
    expect(sameAsDefaults(DEFAULT_WATCH.map((e) => ({ ...e })))).toBe(true)
    expect(sameAsDefaults(DEFAULT_WATCH.slice(1))).toBe(false)
    expect(DEFAULT_WATCH).toHaveLength(10)
  })
  it('settings default Terminal builds on and keep it on for old files', () => {
    expect(normalize({}).adapters.build).toBe(true)
    expect(normalize({ adapters: { cursor: false } }).adapters).toEqual({ cursor: false, build: true })
    expect(normalize({ adapters: { cursor: true, build: false } }).adapters.build).toBe(false)
  })
})

describe('verse interval clamp (Kit bug 2)', () => {
  it('raises max to min and min to 5 before saving', () => {
    expect(normalize({ verseIntervalSec: [60, 30] }).verseIntervalSec).toEqual([60, 60])
    expect(normalize({ verseIntervalSec: [1, 2] }).verseIntervalSec).toEqual([5, 5])
  })
})
