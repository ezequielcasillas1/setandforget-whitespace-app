/** What a practice whiteout stands for: seeing whitespace, not testing the programs. */
export const PRACTICE_INTENTION =
  'Whiteout stands for seeing the whitespace — practicing the art of its intentions, without the programs.'

export const PRACTICE_LABEL = 'WhiteSpace me'

export function isPracticeShow(payload: { practice?: boolean } | null | undefined): boolean {
  return payload?.practice === true
}
