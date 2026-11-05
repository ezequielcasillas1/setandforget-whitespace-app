import { describe, expect, it } from 'vitest'
import { isPracticeShow, PRACTICE_INTENTION, PRACTICE_LABEL } from '../src/practice/intention'

describe('practice whiteout', () => {
  it('names seeing the whitespace, not a program test', () => {
    expect(PRACTICE_LABEL).toBe('WhiteSpace me')
    expect(PRACTICE_INTENTION).toContain('seeing the whitespace')
    expect(PRACTICE_INTENTION).toContain('practicing the art of its intentions')
    expect(PRACTICE_INTENTION).toContain('without the programs')
  })

  it('treats only an explicit practice payload as practice', () => {
    expect(isPracticeShow({ practice: true })).toBe(true)
    expect(isPracticeShow({ practice: false })).toBe(false)
    expect(isPracticeShow({})).toBe(false)
    expect(isPracticeShow(null)).toBe(false)
  })
})
