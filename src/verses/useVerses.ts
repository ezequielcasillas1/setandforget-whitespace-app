import { useCallback, useRef, useState } from 'react'
import type { PhaseEvent } from '../breathing/breath-engine'
import { seeded } from '../rng'
import { FALLBACK_VERSE, getRefs, getTranslations, getVerse, type ApiVerse } from './client'
import { RefDeck, TranslationRotor, VerseClock, type VerseTiming } from './scheduler'
import { toRotorList } from './translations'

type Parts = { deck: RefDeck; rotor: TranslationRotor; clock: VerseClock }

export function useVerses(seed: number, timing: VerseTiming) {
  const [verse, setVerse] = useState<ApiVerse | null>(null)
  const parts = useRef<Parts | null>(null)
  const clockRef = useRef<VerseClock | null>(null)
  const token = useRef(0)

  const showNext = useCallback(async () => {
    const my = ++token.current
    const p = parts.current
    if (p) {
      for (let attempt = 0; attempt < 4; attempt++) {
        const ref = p.deck.next()
        const t = p.rotor.next(ref)
        try {
          const v = await getVerse(ref, t.abbr)
          if (my === token.current) setVerse(v)
          return
        } catch { /* 404/422 or API down: try the next ref + translation */ }
      }
    }
    // API down or not running (e.g. plain-browser dev): never blank.
    if (my === token.current) setVerse(FALLBACK_VERSE)
  }, [])

  const start = useCallback(async () => {
    const rng = seeded(seed)
    clockRef.current = new VerseClock(timing, rng)
    clockRef.current.start(0)
    setVerse(null)
    try {
      const [refs, translations] = await Promise.all([getRefs(), getTranslations()])
      parts.current = refs.length && translations.length
        ? { deck: new RefDeck(refs, rng), rotor: new TranslationRotor(toRotorList(translations)), clock: clockRef.current }
        : null
    } catch { parts.current = null }
    await showNext()
  }, [seed, timing, showNext])

  const onPhase = useCallback((e: PhaseEvent, elapsed: number) => {
    if (clockRef.current?.onPhase(e, elapsed)) void showNext()
  }, [showNext])

  const stop = useCallback(() => { parts.current = null; clockRef.current = null; token.current++ }, [])
  return { verse, start, onPhase, stop }
}
