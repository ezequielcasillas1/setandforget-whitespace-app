// The API owns the abbr→name table (card §7.1). This only turns /v1/translations into the rotor's list.
import type { ApiTranslation } from './client'
import type { Translation } from './scheduler'

export const toRotorList = (list: ApiTranslation[]): Translation[] =>
  list.map((t) => ({ abbr: t.abbr, otOnly: t.testaments.length === 1 && t.testaments[0].toUpperCase() === 'OT' }))
