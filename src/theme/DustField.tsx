// Dust theme: AER BreathEcosystem's motes (copied from /workspace/apps/aer-breathe/app/src/components/BreathEcosystem.tsx,
// Oct 27 2026) with the rings, cross-shaped "line" motes, gradients, membrane and bursts removed (spec: no lines, no
// gradient rings). Motes are drawn as pixel-snapped squares (dithered), faint ink on white, slowed down, and breathe
// with the same breath value as the circle.
import { useEffect, useRef } from 'react'

const TAU = Math.PI * 2
const random = (seed: number) => {
  const x = Math.sin(seed * 999.91) * 43758.5453
  return x - Math.floor(x)
}
const SLOW = 0.5

export default function DustField({ breathRef, reduced }: { breathRef: React.RefObject<number>; reduced: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = canvasRef.current
    const ctx = canvas?.getContext('2d')
    if (!canvas || !ctx) return
    let width = 0, height = 0, animation = 0
    const motes = Array.from({ length: reduced ? 40 : 92 }, (_, i) => ({
      angle: random(i + 1) * TAU,
      distance: 0.28 + random(i + 15) * 0.7,
      size: 0.45 + random(i + 42) * 1.65,
      speed: (0.025 + random(i + 83) * 0.075) * (random(i + 6) > 0.5 ? 1 : -1),
      drift: random(i + 112) * TAU,
      alpha: 0.2 + random(i + 52) * 0.7,
      layer: random(i + 99),
    }))
    const resize = () => {
      const rect = canvas.getBoundingClientRect()
      width = rect.width; height = rect.height
      canvas.width = Math.round(width); canvas.height = Math.round(height) // 1:1 pixels for the dithered look
    }
    const observer = new ResizeObserver(resize)
    observer.observe(canvas)
    resize()
    const draw = (now: number) => {
      const time = reduced ? 0 : (now / 1000) * SLOW
      const breath = breathRef.current ?? 0
      ctx.clearRect(0, 0, width, height)
      const cx = width / 2, cy = height / 2, unit = Math.min(width, height)
      motes.forEach((mote, i) => {
        const angle = mote.angle + time * mote.speed
        const r = unit * 0.41 * mote.distance * (0.88 + breath * 0.18)
        const flutter = reduced ? 0 : Math.sin(time * 0.65 + mote.drift) * (3 + mote.layer * 5)
        const x = cx + Math.cos(angle) * (r + flutter)
        const y = cy + Math.sin(angle) * (r * 0.82 + flutter) + Math.sin(time * 0.24 + i) * 3
        const twinkle = 0.55 + Math.sin(time * (0.6 + mote.layer) + mote.drift) * 0.35
        const alpha = Math.max(0.04, mote.alpha * twinkle) * 0.35 // faint
        const s = mote.size > 1.2 ? 2 : 1
        ctx.fillStyle = `rgba(29,29,27,${alpha})`
        ctx.fillRect(Math.round(x), Math.round(y), s, s)
      })
      if (!reduced) animation = requestAnimationFrame(draw)
    }
    animation = requestAnimationFrame(draw)
    // Reduced motion: draw one static frame, then redraw slowly only so the field still exists.
    const still = reduced ? setInterval(() => requestAnimationFrame(draw), 1000) : 0
    return () => { observer.disconnect(); cancelAnimationFrame(animation); if (still) clearInterval(still) }
  }, [breathRef, reduced])
  return <canvas ref={canvasRef} aria-hidden="true" />
}
