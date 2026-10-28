// Copied from /workspace/apps/aer-breathe/app/src/hooks/useBreathing.ts (PATTERNS), Oct 27 2026.
// The first three are unchanged. The last three are new per docs/specs/01-mvp.md and
// docs/architecture/01-mvp-card.md §6. Their notes are placeholders (open question).
import type { BreathPattern } from './breath-engine'

export const PATTERNS: BreathPattern[] = [
  {
    id: 'coherence',
    name: 'Coherence',
    note: 'A gentle, balanced rhythm for finding your center.',
    phases: [
      { kind: 'inhale', duration: 5, label: 'Breathe in' },
      { kind: 'exhale', duration: 5, label: 'Breathe out' },
    ],
  },
  {
    id: 'calm',
    name: 'Deep calm',
    note: 'A longer exhale to help the body soften.',
    phases: [
      { kind: 'inhale', duration: 4, label: 'Breathe in' },
      { kind: 'hold', duration: 4, label: 'Hold softly' },
      { kind: 'exhale', duration: 6, label: 'Breathe out' },
      { kind: 'hold', duration: 2, label: 'Rest here' },
    ],
  },
  {
    id: 'box',
    name: 'Box breathing',
    note: 'Four equal sides for focus and steadiness.',
    phases: [
      { kind: 'inhale', duration: 4, label: 'Breathe in' },
      { kind: 'hold', duration: 4, label: 'Hold softly' },
      { kind: 'exhale', duration: 4, label: 'Breathe out' },
      { kind: 'hold', duration: 4, label: 'Rest here' },
    ],
  },
  {
    id: '478',
    name: '4-7-8',
    note: '4 in, 7 hold, 8 out. (Placeholder note, TBD.)',
    phases: [
      { kind: 'inhale', duration: 4, label: 'Breathe in' },
      { kind: 'hold', duration: 7, label: 'Hold softly' },
      { kind: 'exhale', duration: 8, label: 'Breathe out' },
    ],
  },
  {
    id: 'extended-exhale',
    name: 'Extended exhale',
    note: '4 in, 8 out. (Placeholder note, TBD.)',
    phases: [
      { kind: 'inhale', duration: 4, label: 'Breathe in' },
      { kind: 'exhale', duration: 8, label: 'Breathe out' },
    ],
  },
  {
    id: 'sigh',
    name: 'Physiological sigh',
    note: '2 in, 1 more in, 6 out. (Placeholder note, TBD.)',
    phases: [
      { kind: 'inhale', duration: 2, label: 'Breathe in' },
      { kind: 'inhale', duration: 1, label: 'Top up' },
      { kind: 'exhale', duration: 6, label: 'Long sigh out' },
    ],
  },
]

/** Bad or missing saved id falls back to Coherence (UX packet §4, proposed). */
export function findPattern(id: string | undefined): BreathPattern {
  return PATTERNS.find((p) => p.id === id) ?? PATTERNS[0]
}
