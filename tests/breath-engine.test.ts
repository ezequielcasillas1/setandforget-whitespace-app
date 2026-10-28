import { describe, expect, it } from 'vitest'
import { cycleDuration, getBreathValue, initialState, simulate, step } from '../src/breathing/breath-engine'
import { PATTERNS, findPattern } from '../src/breathing/patterns'

const byId = (id: string) => PATTERNS.find((p) => p.id === id)!
const timings = (id: string) => byId(id).phases.map((p) => `${p.kind}:${p.duration}`)

describe('exercises', () => {
  it('has exactly six, AER three first, unchanged', () => {
    expect(PATTERNS.map((p) => p.id)).toEqual(['coherence', 'calm', 'box', '478', 'extended-exhale', 'sigh'])
    expect(PATTERNS.map((p) => p.name)).toEqual(['Coherence', 'Deep calm', 'Box breathing', '4-7-8', 'Extended exhale', 'Physiological sigh'])
  })
  it('uses spec timings', () => {
    expect(timings('coherence')).toEqual(['inhale:5', 'exhale:5'])
    expect(timings('calm')).toEqual(['inhale:4', 'hold:4', 'exhale:6', 'hold:2'])
    expect(timings('box')).toEqual(['inhale:4', 'hold:4', 'exhale:4', 'hold:4'])
    expect(timings('478')).toEqual(['inhale:4', 'hold:7', 'exhale:8'])
    expect(timings('extended-exhale')).toEqual(['inhale:4', 'exhale:8'])
    expect(timings('sigh')).toEqual(['inhale:2', 'inhale:1', 'exhale:6'])
  })
  it('cycle durations', () => {
    expect(PATTERNS.map(cycleDuration)).toEqual([10, 16, 16, 19, 12, 9])
  })
  it('falls back to Coherence for unknown ids', () => {
    expect(findPattern('nope').id).toBe('coherence')
    expect(findPattern(undefined).id).toBe('coherence')
  })
})

describe('step model', () => {
  it('clamps delta to 0.1 s like AER', () => {
    const r = step(byId('coherence'), { ...initialState, running: true }, 5, Infinity)
    expect(r.state.elapsed).toBeCloseTo(0.1)
    expect(r.state.phaseIndex).toBe(0)
  })

  for (const p of PATTERNS) {
    it(`${p.id}: phase boundaries land at the right times`, () => {
      let t = 0
      p.phases.forEach((ph, i) => {
        const mid = simulate(p, t + ph.duration / 2)
        expect(mid.state.phaseIndex).toBe(i)
        t += ph.duration
      })
      const after = simulate(p, cycleDuration(p) + 0.05)
      expect(after.state.phaseIndex).toBe(0)
      expect(after.state.cycle).toBe(2)
    })
    it(`${p.id}: emits one event per phase change, cycle-start inhale once per cycle`, () => {
      const { events } = simulate(p, cycleDuration(p) * 3 + 0.05)
      expect(events.length).toBe(p.phases.length * 3)
      expect(events.filter((e) => e.kind === 'inhale' && e.phaseIndex === 0).length).toBe(3)
    })
  }

  it('breath value: inhale rises 0→1, exhale falls 1→0, holds follow previous phase', () => {
    const box = byId('box')
    expect(simulate(box, 0.05).value).toBeLessThan(0.01)
    expect(simulate(box, 2).value).toBeCloseTo(0.5, 1)
    expect(simulate(box, 5).value).toBe(1) // hold after inhale
    expect(simulate(box, 10).value).toBeCloseTo(0.5, 1)
    expect(simulate(box, 14).value).toBe(0) // rest after exhale
  })

  it('physiological sigh: second inhale is additive (never drops back to 0)', () => {
    const sigh = byId('sigh')
    let prev = -1
    for (let t = 0.05; t < 3; t += 0.05) {
      const v = simulate(sigh, t).value
      expect(v).toBeGreaterThanOrEqual(prev - 1e-9)
      prev = v
    }
    expect(simulate(sigh, 2.95).value).toBeGreaterThan(0.99)
    expect(simulate(sigh, 3 + 3).value).toBeCloseTo(0.5, 1)
  })

  it('first phase of AER patterns matches AER getBreathValue', () => {
    const c = byId('coherence')
    const s = { ...initialState, phaseElapsed: 2.5 }
    expect(getBreathValue(c, s)).toBeCloseTo(0.5)
  })

  it('stops at the session cap (20 min)', () => {
    const r = simulate(byId('coherence'), 20 * 60 + 1, 0.1, 20 * 60)
    expect(r.state.complete).toBe(true)
    expect(r.state.running).toBe(false)
    expect(r.state.elapsed).toBe(1200)
  })
})
