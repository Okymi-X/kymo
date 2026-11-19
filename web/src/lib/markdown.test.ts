import { describe, expect, it } from 'vitest'
import { parseInline, parseMarkdown, safeHref } from './markdown'

describe('markdown', () => {
  it('keeps a plain description as it was: one paragraph, line breaks kept', () => {
    expect(parseMarkdown('Find the flag.\nIt is hidden.')).toEqual([
      { t: 'p', c: [{ t: 'text', v: 'Find the flag.' }, { t: 'br' }, { t: 'text', v: 'It is hidden.' }] },
    ])
  })

  it('reads emphasis, code and links', () => {
    expect(parseInline('a **b** *c* `d` [e](https://x.test/p)')).toEqual([
      { t: 'text', v: 'a ' },
      { t: 'strong', c: [{ t: 'text', v: 'b' }] },
      { t: 'text', v: ' ' },
      { t: 'em', c: [{ t: 'text', v: 'c' }] },
      { t: 'text', v: ' ' },
      { t: 'code', v: 'd' },
      { t: 'text', v: ' ' },
      { t: 'link', href: 'https://x.test/p', c: [{ t: 'text', v: 'e' }] },
    ])
  })

  it('does not treat snake_case words as italic', () => {
    expect(parseInline('user_flag_one')).toEqual([{ t: 'text', v: 'user_flag_one' }])
  })

  it('reads headings, lists, quotes, rules and fenced code', () => {
    const blocks = parseMarkdown('# Title\n\n- one\n- two\n\n1. a\n2. b\n\n> note\n\n---\n\n```sh\nnmap -sV 10.0.0.1\n```')
    expect(blocks.map((b) => b.t)).toEqual(['h', 'list', 'list', 'quote', 'hr', 'code'])
    expect(blocks[1]).toMatchObject({ t: 'list', ordered: false })
    expect(blocks[2]).toMatchObject({ t: 'list', ordered: true })
    expect(blocks[5]).toEqual({ t: 'code', v: 'nmap -sV 10.0.0.1', lang: 'sh' })
  })

  it('never makes a link of an unsafe address, and never makes HTML', () => {
    expect(safeHref('javascript:alert(1)')).toBeNull()
    expect(safeHref('data:text/html,x')).toBeNull()
    expect(safeHref('https://ok.test')).toBe('https://ok.test')
    expect(parseInline('[x](javascript:alert(1))')).toEqual([{ t: 'text', v: '[x](javascript:alert(1))' }])
    // A tag stays plain text: the renderer only builds React elements, never HTML.
    expect(parseInline('<script>alert(1)</script>')).toEqual([{ t: 'text', v: '<script>alert(1)</script>' }])
  })
})
