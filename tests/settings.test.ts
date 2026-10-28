import { describe, expect, it } from 'vitest'
import { normalize } from '../src/settings'

describe('display switches (card §17)', () => {
  it('default both on', () => expect(normalize({}).display).toEqual({ verses: true, breathing: true }))
  it('keeps an off switch and fills the missing one', () =>
    expect(normalize({ display: { verses: false } as never }).display).toEqual({ verses: false, breathing: true }))
  it('both off stays both off', () =>
    expect(normalize({ display: { verses: false, breathing: false } }).display).toEqual({ verses: false, breathing: false }))
})

import { hookStatusText } from '../src/windows/Settings'
describe('AI builder rows (slice 03b)', () => {
  const h = (o: object) => ({ id: 'claude', found: true, installed: false, trustPending: false, ...o })
  it('defaults: all on except Grok Bot', () => expect(normalize({}).adapters).toMatchObject({ claude: true, codex: true, copilot: true, cursor: true, windsurf: true, grokbot: false }))
  it('status lines', () => {
    expect(hookStatusText('Claude Code', h({ found: false }), false)).toBe("Can't find Claude Code on this PC")
    expect(hookStatusText('Claude Code', h({}), false)).toBe('Hook not installed')
    expect(hookStatusText('Claude Code', h({ installed: true }), false)).toBe('Hook installed')
    expect(hookStatusText('OpenAI Codex', h({ id: 'codex', installed: true, trustPending: true }), false)).toBe('Hook installed, waiting for trust in Codex')
    expect(hookStatusText('Cursor', h({}), true)).toBe("Couldn't install. Your file wasn't changed.")
  })
})
