import type { ReactNode } from 'react'
import { parseMarkdown, type Block, type Inline } from '../lib/markdown'

function inline(nodes: Inline[]): ReactNode[] {
  return nodes.map((n, i) => {
    switch (n.t) {
      case 'text':
        return n.v
      case 'br':
        return <br key={i} />
      case 'code':
        return (
          <code key={i} className="rounded bg-bg px-1 py-0.5 font-mono text-[0.85em]">
            {n.v}
          </code>
        )
      case 'strong':
        return <strong key={i}>{inline(n.c)}</strong>
      case 'em':
        return <em key={i}>{inline(n.c)}</em>
      case 'link':
        return (
          <a key={i} href={n.href} target="_blank" rel="noopener noreferrer nofollow" className="text-accent underline">
            {inline(n.c)}
          </a>
        )
    }
  })
}

function block(b: Block, i: number): ReactNode {
  switch (b.t) {
    case 'p':
      return <p key={i}>{inline(b.c)}</p>
    case 'h':
      return (
        <p key={i} role="heading" aria-level={Math.min(b.level + 2, 6)} className={b.level <= 2 ? 'text-base font-semibold' : 'font-semibold'}>
          {inline(b.c)}
        </p>
      )
    case 'code':
      return (
        <pre key={i} className="overflow-x-auto rounded-control border border-line bg-bg p-3 font-mono text-xs leading-relaxed">
          <code>{b.v}</code>
        </pre>
      )
    case 'list': {
      const items = b.items.map((it, j) => <li key={j}>{inline(it)}</li>)
      return b.ordered ? (
        <ol key={i} className="list-decimal pl-5">
          {items}
        </ol>
      ) : (
        <ul key={i} className="list-disc pl-5">
          {items}
        </ul>
      )
    }
    case 'quote':
      return (
        <blockquote key={i} className="flex flex-col gap-2 border-l-2 border-line pl-3 text-muted">
          {b.c.map(block)}
        </blockquote>
      )
    case 'hr':
      return <hr key={i} className="border-line" />
  }
}

/** Challenge text written in Markdown, shown as safe elements (never raw HTML). */
export function Markdown({ text, className = '' }: { text: string; className?: string }) {
  return <div className={`flex flex-col gap-2 break-words text-sm leading-relaxed ${className}`}>{parseMarkdown(text).map(block)}</div>
}
