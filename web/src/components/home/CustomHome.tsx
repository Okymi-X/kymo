import DOMPurify from 'dompurify'
import { useEffect, useRef } from 'react'
import { useNavigate } from 'react-router'

const TAGS = [
  'a', 'abbr', 'article', 'aside', 'b', 'blockquote', 'br', 'caption', 'code', 'col', 'colgroup', 'dd', 'details', 'div', 'dl', 'dt', 'em',
  'figcaption', 'figure', 'footer', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'header', 'hr', 'i', 'img', 'li', 'main', 'mark', 'nav', 'ol', 'p', 'pre',
  'section', 'small', 'span', 'strong', 'sub', 'summary', 'sup', 'table', 'tbody', 'td', 'th', 'thead', 'tr', 'u', 'ul',
]
const ATTRS = ['class', 'id', 'title', 'role', 'aria-label', 'href', 'src', 'alt', 'width', 'height']

/** What every custom page starts from, so plain HTML looks reasonable before the organizer's CSS. */
const BASE_CSS = `
:host { display: block; }
.kymo-home { font: inherit; color: inherit; line-height: 1.6; overflow-wrap: anywhere; }
.kymo-home a { color: var(--color-accent); }
.kymo-home img { max-width: 100%; height: auto; }
.kymo-home h1, .kymo-home h2, .kymo-home h3 { line-height: 1.2; letter-spacing: -0.02em; }
`

/** The organizer's own home page. The HTML is cleaned again here (the server already cleaned it) and shown inside a
 *  shadow root: their CSS cannot touch the rest of the app, the app's styles do not leak in, and our colour
 *  variables still reach through, so `var(--color-accent)` works. */
export function CustomHome({ html, css }: { html: string; css: string }) {
  const host = useRef<HTMLDivElement>(null)
  const navigate = useNavigate()
  useEffect(() => {
    const el = host.current
    if (!el) return
    const root = el.shadowRoot ?? el.attachShadow({ mode: 'open' })
    const clean = DOMPurify.sanitize(html, { ALLOWED_TAGS: TAGS, ALLOWED_ATTR: ATTRS, ALLOW_DATA_ATTR: false })
    const style = document.createElement('style')
    style.textContent = BASE_CSS + css
    const page = document.createElement('div')
    page.className = 'kymo-home'
    page.innerHTML = clean
    root.replaceChildren(style, page)
  }, [html, css])
  // A link to a page of this site moves inside the app instead of reloading it.
  const onClick = (e: React.MouseEvent) => {
    const anchor = e.nativeEvent.composedPath().find((n): n is HTMLAnchorElement => n instanceof HTMLAnchorElement)
    const href = anchor?.getAttribute('href')
    if (!anchor || !href || !href.startsWith('/') || href.startsWith('//')) return
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return
    e.preventDefault()
    navigate(href)
  }
  return <div ref={host} onClick={onClick} />
}
