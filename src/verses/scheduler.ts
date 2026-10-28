// Verse timing (card §7, Cheke's answer): verse 1 at breathing start, then every 45–60 s,
// changing only on the cycle-start inhale (phaseIndex 0). Shuffled refs, no repeats until all used.
// Translations rotate round-robin per change, skipping OT-only ones (JPS) for NT refs.
import type { PhaseEvent } from '../breathing/breath-engine'
import { isNewTestament, parseRef } from './refs'
export type Translation = { abbr: string; otOnly?: boolean }

export type Rng = () => number

export function shuffle<T>(items: T[], rng: Rng = Math.random): T[] {
  const a = [...items]
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1))
    ;[a[i], a[j]] = [a[j], a[i]]
  }
  return a
}

export class RefDeck {
  private order: string[] = []
  private pos = 0
  constructor(private refs: string[], private rng: Rng = Math.random) { this.reshuffle() }
  private reshuffle() {
    const last = this.order[this.order.length - 1]
    this.order = shuffle(this.refs, this.rng)
    if (this.order.length > 1 && this.order[0] === last) this.order.push(this.order.shift()!)
    this.pos = 0
  }
  next(): string {
    if (this.pos >= this.order.length) this.reshuffle()
    return this.order[this.pos++]
  }
}

export class TranslationRotor {
  private i = -1
  constructor(private list: Translation[]) {}
  next(ref: string): Translation {
    const parsed = parseRef(ref)
    const nt = parsed ? isNewTestament(parsed.book) : false
    for (let k = 0; k < this.list.length; k++) {
      this.i = (this.i + 1) % this.list.length
      const t = this.list[this.i]
      if (!(nt && t.otOnly)) return t
    }
    return this.list[0]
  }
}

export type VerseTiming = { minSec: number; maxSec: number }
export const DEFAULT_TIMING: VerseTiming = { minSec: 45, maxSec: 60 }

/** Decides *when* to change. Feed it breathing time and phase events. */
export class VerseClock {
  private dueAt = Infinity
  constructor(private timing: VerseTiming = DEFAULT_TIMING, private rng: Rng = Math.random) {}
  private interval() {
    const { minSec, maxSec } = this.timing
    return minSec + (maxSec - minSec) * this.rng()
  }
  /** Breathing started: show verse 1 now. Returns true (caller shows a verse). */
  start(elapsed = 0): true {
    this.dueAt = elapsed + this.interval()
    return true
  }
  /** Returns true when a phase event should change the verse. */
  onPhase(e: PhaseEvent, elapsed: number): boolean {
    if (e.kind !== 'inhale' || e.phaseIndex !== 0) return false
    if (elapsed < this.dueAt) return false
    this.dueAt = elapsed + this.interval()
    return true
  }
  get nextDue() { return this.dueAt }
}
