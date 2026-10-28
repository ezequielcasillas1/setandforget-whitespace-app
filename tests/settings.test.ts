import { describe, expect, it } from 'vitest'
import { normalize } from '../src/settings'

describe('display switches (card §17)', () => {
  it('default both on', () => expect(normalize({}).display).toEqual({ verses: true, breathing: true }))
  it('keeps an off switch and fills the missing one', () =>
    expect(normalize({ display: { verses: false } as never }).display).toEqual({ verses: false, breathing: true }))
  it('both off stays both off', () =>
    expect(normalize({ display: { verses: false, breathing: false } }).display).toEqual({ verses: false, breathing: false }))
})
