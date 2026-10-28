// Verses come only from the local KBS verse API (card rev 2 §7), via Rust commands.
// The webview itself never makes network calls.
import { invoke, isTauri } from '../tauri'

export type ApiVerse = {
  ref: string
  display: string
  text: string
  translation: { abbr: string; name: string }
  source: string
}
export type ApiTranslation = { abbr: string; name: string; testaments: string[] }

/** Bundled fallback (card §7.5, UX §4): KJV John 14:27, always with its version label. */
export const FALLBACK_VERSE: ApiVerse = {
  ref: 'JHN.14.27',
  display: 'John 14:27',
  text: 'Peace I leave with you, my peace I give unto you: not as the world giveth, give I unto you. Let not your heart be troubled, neither let it be afraid.',
  translation: { abbr: 'KJV', name: 'King James Version' },
  source: 'Free Use Bible API (HelloAO)',
}

export const versionLabel = (v: ApiVerse) => `${v.translation.abbr} · ${v.translation.name}`

export async function getRefs(): Promise<string[]> {
  if (!isTauri()) return []
  return invoke<string[]>('verse_refs')
}
export async function getTranslations(): Promise<ApiTranslation[]> {
  if (!isTauri()) return []
  return invoke<ApiTranslation[]>('verse_translations')
}
export async function getVerse(ref: string, translation: string): Promise<ApiVerse> {
  if (!isTauri()) throw new Error('no local verse API outside the app')
  return invoke<ApiVerse>('verse_get', { reference: ref, translation })
}
