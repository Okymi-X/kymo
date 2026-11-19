// A small, dependency free Markdown reader for challenge descriptions. It builds a tree that the
// Markdown component turns into React elements, so nothing here ever becomes raw HTML: a
// description cannot inject markup, script or remote images. Supported: paragraphs (a single line
// break stays a line break, so plain descriptions look as they always did), # headings, **bold**,
// *italic* or _italic_, `code`, fenced code blocks, - and 1. lists, > quotes, --- rules and
// [text](http, https or mailto links).

export type Inline =
  | { t: 'text'; v: string }
  | { t: 'br' }
  | { t: 'code'; v: string }
  | { t: 'strong'; c: Inline[] }
  | { t: 'em'; c: Inline[] }
  | { t: 'link'; href: string; c: Inline[] }

export type Block =
  | { t: 'p'; c: Inline[] }
  | { t: 'h'; level: number; c: Inline[] }
  | { t: 'code'; v: string; lang: string }
  | { t: 'list'; ordered: boolean; items: Inline[][] }
  | { t: 'quote'; c: Block[] }
  | { t: 'hr' }

/** Only these addresses become links; anything else (javascript:, data:, ...) stays as text. */
export function safeHref(url: string): string | null {
  const u = url.trim()
  return /^(https?:\/\/|mailto:)[^\s<>"']+$/i.test(u) ? u : null
}

const isWord = (c: string | undefined) => c !== undefined && /[\p{L}\p{N}]/u.test(c)

export function parseInline(s: string): Inline[] {
  const out: Inline[] = []
  let text = ''
  const flush = () => {
    if (text) out.push({ t: 'text', v: text })
    text = ''
  }
  let i = 0
  while (i < s.length) {
    const ch = s[i]
    if (ch === '\\' && i + 1 < s.length && /[\\`*_{}[\]()#+\-.!>~|]/.test(s[i + 1])) {
      text += s[i + 1]
      i += 2
      continue
    }
    if (ch === '\n') {
      flush()
      out.push({ t: 'br' })
      i += 1
      continue
    }
    if (ch === '`') {
      const end = s.indexOf('`', i + 1)
      if (end > i + 1) {
        flush()
        out.push({ t: 'code', v: s.slice(i + 1, end) })
        i = end + 1
        continue
      }
    }
    if (ch === '*' && s[i + 1] === '*') {
      const end = s.indexOf('**', i + 2)
      if (end > i + 2 && !/^\s|\s$/.test(s.slice(i + 2, end))) {
        flush()
        out.push({ t: 'strong', c: parseInline(s.slice(i + 2, end)) })
        i = end + 2
        continue
      }
    }
    if (ch === '*' || ch === '_') {
      const end = s.indexOf(ch, i + 1)
      const inner = end > i + 1 ? s.slice(i + 1, end) : ''
      const wordSafe = ch === '*' || (!isWord(s[i - 1]) && !isWord(s[end + 1]))
      if (inner && !/^\s|\s$/.test(inner) && !inner.includes('\n') && wordSafe) {
        flush()
        out.push({ t: 'em', c: parseInline(inner) })
        i = end + 1
        continue
      }
    }
    if (ch === '[') {
      const mid = s.indexOf('](', i + 1)
      const close = mid > 0 ? s.indexOf(')', mid + 2) : -1
      if (mid > i + 1 && close > mid + 2) {
        const href = safeHref(s.slice(mid + 2, close))
        if (href) {
          flush()
          out.push({ t: 'link', href, c: parseInline(s.slice(i + 1, mid)) })
          i = close + 1
          continue
        }
      }
    }
    text += ch
    i += 1
  }
  flush()
  return out
}

const FENCE = /^\s{0,3}(```+|~~~+)\s*([\w+-]*)\s*$/
const HEADING = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/
const RULE = /^\s{0,3}([-*_])(\s*\1){2,}\s*$/
const BULLET = /^\s{0,3}[-*+]\s+(.*)$/
const NUMBER = /^\s{0,3}\d{1,9}[.)]\s+(.*)$/
const QUOTE = /^\s{0,3}>\s?(.*)$/

const startsBlock = (line: string) =>
  FENCE.test(line) || HEADING.test(line) || RULE.test(line) || BULLET.test(line) || NUMBER.test(line) || QUOTE.test(line)

export function parseMarkdown(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, '\n').split('\n')
  const blocks: Block[] = []
  let i = 0
  while (i < lines.length) {
    const line = lines[i]
    if (line.trim() === '') {
      i += 1
      continue
    }
    const fence = FENCE.exec(line)
    if (fence) {
      const body: string[] = []
      i += 1
      while (i < lines.length && !lines[i].trim().startsWith(fence[1].slice(0, 3))) {
        body.push(lines[i])
        i += 1
      }
      i += 1
      blocks.push({ t: 'code', v: body.join('\n'), lang: fence[2] })
      continue
    }
    const heading = HEADING.exec(line)
    if (heading) {
      blocks.push({ t: 'h', level: heading[1].length, c: parseInline(heading[2]) })
      i += 1
      continue
    }
    if (RULE.test(line)) {
      blocks.push({ t: 'hr' })
      i += 1
      continue
    }
    if (QUOTE.test(line)) {
      const inner: string[] = []
      while (i < lines.length && QUOTE.test(lines[i])) {
        inner.push(QUOTE.exec(lines[i])![1])
        i += 1
      }
      blocks.push({ t: 'quote', c: parseMarkdown(inner.join('\n')) })
      continue
    }
    const bullet = BULLET.test(line)
    if (bullet || NUMBER.test(line)) {
      const items: Inline[][] = []
      const marker = bullet ? BULLET : NUMBER
      while (i < lines.length) {
        const m = marker.exec(lines[i])
        if (!m) break
        let item = m[1]
        i += 1
        // A more indented line continues the item.
        while (i < lines.length && /^\s{2,}\S/.test(lines[i]) && !marker.test(lines[i])) {
          item += '\n' + lines[i].trim()
          i += 1
        }
        items.push(parseInline(item))
      }
      blocks.push({ t: 'list', ordered: !bullet, items })
      continue
    }
    const para: string[] = []
    while (i < lines.length && lines[i].trim() !== '' && (para.length === 0 || !startsBlock(lines[i]))) {
      para.push(lines[i])
      i += 1
    }
    blocks.push({ t: 'p', c: parseInline(para.join('\n')) })
  }
  return blocks
}
