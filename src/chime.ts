// One soft built-in chime (spec: Remy's calls). Synthesised with WebAudio, so no sound file is bundled.
export function playChime(volume: number): Promise<void> {
  return new Promise((resolve) => {
    try {
      const ctx = new AudioContext()
      const now = ctx.currentTime
      ;[660, 990].forEach((f, i) => {
        const osc = ctx.createOscillator()
        const gain = ctx.createGain()
        osc.type = 'sine'
        osc.frequency.value = f
        gain.gain.setValueAtTime(0, now)
        gain.gain.linearRampToValueAtTime(volume * (i ? 0.35 : 0.6), now + 0.02)
        gain.gain.exponentialRampToValueAtTime(0.0001, now + 1.2)
        osc.connect(gain).connect(ctx.destination)
        osc.start(now)
        osc.stop(now + 1.25)
      })
      setTimeout(() => { ctx.close(); resolve() }, 350)
    } catch { resolve() }
  })
}
