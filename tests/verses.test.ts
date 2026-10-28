import { describe, expect, it } from 'vitest'
import { initialState, step, type BreathState } from '../src/breathing/breath-engine'
import { PATTERNS } from '../src/breathing/patterns'
import { seeded } from '../src/rng'
import { FALLBACK_VERSE, versionLabel } from '../src/verses/client'
import { isNewTestament, parseRef, refLabel } from '../src/verses/refs'
import { RefDeck, TranslationRotor, VerseClock } from '../src/verses/scheduler'
import { toRotorList } from '../src/verses/translations'

const REFS = ['JHN.3.16', 'PSA.23.1', 'EPH.2.8-9', 'ROM.8.28', 'ISA.41.10', 'PHP.4.6-7', 'PRO.3.5-6', 'MAT.11.28']
const ABBRS = ['KJV','WEB','ASV','Darby','YLT','BSB','GNV','Webster','RV','BBE','DRA','WEBU','JPS']
const TRANSLATIONS = toRotorList(ABBRS.map((abbr) => ({ abbr, name: abbr, testaments: abbr === 'JPS' ? ['OT'] : ['OT', 'NT'] })))

describe('verse clock', () => {
  for (const p of PATTERNS) {
    it(`${p.id}: verse 1 at start, then 45–60 s apart, only on a cycle-start inhale`, () => {
      const clock = new VerseClock({ minSec: 45, maxSec: 60 }, seeded(7))
      const changes: number[] = [0]
      clock.start(0)
      const cyc = p.phases.reduce((a, b) => a + b.duration, 0)
      let st: BreathState = { ...initialState, running: true, started: true }
      for (let i = 0; i < 12000; i++) {
        const r = step(p, st, 0.05, Infinity)
        st = r.state
        for (const e of r.events) if (clock.onPhase(e, st.elapsed)) changes.push(st.elapsed)
      }
      for (let i = 1; i < changes.length; i++) {
        const gap = changes[i] - changes[i - 1]
        expect(gap).toBeGreaterThanOrEqual(45)
        expect(gap).toBeLessThanOrEqual(60 + cyc) // waits for the next inhale
        const m = changes[i] % cyc
        expect(Math.min(m, cyc - m)).toBeLessThan(0.06)
      }
      expect(changes.length).toBeGreaterThan(8)
    })
  }
  it('ignores non-inhale and second-inhale events', () => {
    const clock = new VerseClock({ minSec: 0, maxSec: 0 }, () => 0)
    clock.start(0)
    expect(clock.onPhase({ kind: 'exhale', phaseIndex: 1, cycle: 1 }, 100)).toBe(false)
    expect(clock.onPhase({ kind: 'inhale', phaseIndex: 1, cycle: 1 }, 100)).toBe(false)
    expect(clock.onPhase({ kind: 'inhale', phaseIndex: 0, cycle: 2 }, 100)).toBe(true)
  })
})

describe('refs and deck', () => {
  it('parses ranges and labels', () => {
    expect(parseRef('EPH.2.8-9')).toEqual({ book: 'EPH', chapter: 2, verse: 8, endVerse: 9 })
    expect(refLabel(parseRef('1JN.4.8')!)).toBe('1 John 4:8')
    expect(isNewTestament('JHN')).toBe(true)
    expect(isNewTestament('PSA')).toBe(false)
  })
  it('no repeats until every ref is used', () => {
    const refs = REFS
    const deck = new RefDeck(refs, seeded(1))
    const seen = new Set(Array.from({ length: refs.length }, () => deck.next()))
    expect(seen.size).toBe(refs.length)
  })
  it('same seed gives the same order (one verse across monitors)', () => {
    const a = new RefDeck(REFS, seeded(42)), b = new RefDeck(REFS, seeded(42))
    for (let i = 0; i < 20; i++) expect(a.next()).toBe(b.next())
  })
  it('rotates all 13 translations, skipping JPS for NT', () => {
    const rot = new TranslationRotor(TRANSLATIONS)
    const ot = Array.from({ length: 13 }, () => rot.next('PSA.23.1').abbr)
    expect(new Set(ot).size).toBe(13)
    const nt = Array.from({ length: 24 }, () => rot.next('JHN.3.16').abbr)
    expect(nt).not.toContain('JPS')
  })
})

describe('fallback verse', () => {
  it('is KJV John 14:27 with its version label', () => {
    expect(FALLBACK_VERSE.ref).toBe('JHN.14.27')
    expect(versionLabel(FALLBACK_VERSE)).toBe('KJV · King James Version')
  })
})
