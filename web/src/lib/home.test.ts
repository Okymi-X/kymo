import { describe, expect, it } from 'vitest'
import { parseRules, scoreboardPath } from './home'

describe('parseRules', () => {
  it('reads headings, lists and paragraphs', () => {
    const blocks = parseRules('# Fair play\nBe kind.\nNo sharing flags.\n\n- no scanning\n- no DoS\n\nThat is all.')
    expect(blocks).toEqual([
      { kind: 'heading', text: 'Fair play' },
      { kind: 'paragraph', text: 'Be kind.\nNo sharing flags.' },
      { kind: 'list', items: ['no scanning', 'no DoS'] },
      { kind: 'paragraph', text: 'That is all.' },
    ])
  })

  it('keeps raw html as plain text, never as markup', () => {
    expect(parseRules('<script>x</script>')).toEqual([{ kind: 'paragraph', text: '<script>x</script>' }])
  })

  it('is empty for no text', () => {
    expect(parseRules('')).toEqual([])
    expect(parseRules('  \n\n ')).toEqual([])
  })
})

describe('scoreboardPath', () => {
  it('sends visitors to the public page and players to their own', () => {
    expect(scoreboardPath('jeopardy', 'cup', false)).toBe('/s/cup')
    expect(scoreboardPath('jeopardy', 'cup', true)).toBe('/e/cup?view=scoreboard')
    expect(scoreboardPath('tournament', 'cup', true)).toBe('/t/cup')
    expect(scoreboardPath('tournament', 'a b', false)).toBe('/s/a%20b')
  })
})
