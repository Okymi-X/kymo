import { Check, Copy } from 'lucide-react'
import { isLeagueFormat } from '../lib/labels'
import { useState } from 'react'
import type { Tournament } from '../api/types'
import { defaultEmbedView, type EmbedView } from '../pages/EmbedPage'
import { Field, Select, Button } from './ui'
import { Modal } from './Modal'

type Theme = 'auto' | 'light' | 'dark'

/** Builds the iframe snippet for putting a live view of the tournament on another page. */
export function embedUrl(origin: string, slug: string, options: { view: EmbedView; theme: Theme; transparent: boolean; title: boolean }, defaultView: EmbedView): string {
  const query = new URLSearchParams()
  if (options.view !== defaultView) query.set('view', options.view)
  if (options.theme !== 'auto') query.set('theme', options.theme)
  if (options.transparent) query.set('transparent', '1')
  if (!options.title) query.set('title', '0')
  const suffix = query.toString()
  return `${origin}/embed/${encodeURIComponent(slug)}${suffix ? `?${suffix}` : ''}`
}

export function embedSnippet(url: string, title: string): string {
  const safeTitle = title.replace(/["<>&]/g, ' ').trim()
  return `<iframe src="${url}" title="${safeTitle}" width="100%" height="640" style="border:0" loading="lazy"></iframe>`
}

export function EmbedDialog({ tournament, onClose }: { tournament: Tournament; onClose: () => void }) {
  const fallback = defaultEmbedView(tournament.format)
  const [view, setView] = useState<EmbedView>(fallback)
  const [theme, setTheme] = useState<Theme>('auto')
  const [transparent, setTransparent] = useState(false)
  const [title, setTitle] = useState(true)
  const [copied, setCopied] = useState(false)

  const url = embedUrl(window.location.origin, tournament.slug, { view, theme, transparent, title }, fallback)
  const snippet = embedSnippet(url, tournament.name)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(snippet)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }
  const views: { value: EmbedView; label: string }[] =
    isLeagueFormat(tournament.format)
      ? [{ value: 'standings', label: 'Standings' }, { value: 'bracket', label: 'Rounds' }]
      : tournament.format === 'group_knockout'
        ? [{ value: 'groups', label: 'Groups' }, { value: 'bracket', label: 'Knockout' }, { value: 'standings', label: 'Standings' }]
        : [{ value: 'bracket', label: 'Bracket' }, { value: 'standings', label: 'Standings' }]

  return (
    <Modal label="Embed this tournament" onClose={onClose} width="36rem">
      <div className="flex flex-col gap-5 p-6">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Embed on a website or stream</h2>
          <p className="mt-1 text-sm text-muted">The view updates live. Use a transparent background for a stream overlay.</p>
        </div>
        <div className="grid grid-cols-2 gap-3 max-sm:grid-cols-1">
          <Field label="Show">
            <Select value={view} onChange={(e) => setView(e.target.value as EmbedView)}>
              {views.map((v) => (
                <option key={v.value} value={v.value}>
                  {v.label}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Theme">
            <Select value={theme} onChange={(e) => setTheme(e.target.value as Theme)}>
              <option value="auto">Match the visitor</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </Select>
          </Field>
        </div>
        <div className="flex flex-wrap gap-6 text-sm">
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={transparent} onChange={(e) => setTransparent(e.target.checked)} className="size-4 accent-accent" /> Transparent background
          </label>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={title} onChange={(e) => setTitle(e.target.checked)} className="size-4 accent-accent" /> Show the title
          </label>
        </div>
        <Field label="Code to paste">
          <textarea readOnly value={snippet} rows={4} onFocus={(e) => e.currentTarget.select()} className="rounded-control border border-field-line bg-field p-3 font-mono text-xs" />
        </Field>
        <div className="flex items-center justify-between gap-2">
          <a href={url} target="_blank" rel="noopener noreferrer" className="text-sm text-accent hover:underline">
            Open the preview
          </a>
          <div className="flex gap-2">
            <Button onClick={copy}>
              {copied ? <Check size={16} aria-hidden="true" /> : <Copy size={16} aria-hidden="true" />}
              {copied ? 'Copied' : 'Copy code'}
            </Button>
            <Button variant="primary" onClick={onClose}>
              Done
            </Button>
          </div>
        </div>
      </div>
    </Modal>
  )
}
