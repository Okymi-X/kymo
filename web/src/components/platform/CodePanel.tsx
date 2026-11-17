import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Play } from 'lucide-react'
import { useState, type KeyboardEvent } from 'react'
import { platform } from '../../api/platform'
import type { PlayerChallenge } from '../../api/platformTypes'
import { Button, ErrorText, Select } from '../ui'
import { CodeResult } from './CodeResult'

const draftKey = (slug: string, id: number) => `kymo.code.${slug}.${id}`

function readDraft(slug: string, id: number): { language: string; source: string } | null {
  try {
    const raw = sessionStorage.getItem(draftKey(slug, id))
    return raw ? (JSON.parse(raw) as { language: string; source: string }) : null
  } catch {
    return null
  }
}

/** Where a player writes their solution and runs it against the tests. The draft survives a
 *  closed dialog for the length of the tab, so a slip of the mouse does not lose the code. */
export function CodePanel({ slug, challenge, open }: { slug: string; challenge: PlayerChallenge; open: boolean }) {
  const code = challenge.code
  const client = useQueryClient()
  const saved = readDraft(slug, challenge.id)
  const [language, setLanguage] = useState(saved && code?.languages.some((l) => l.id === saved.language) ? saved.language : (code?.languages[0]?.id ?? ''))
  const [source, setSource] = useState(saved?.source ?? '')
  const keep = (next: { language: string; source: string }) => {
    try {
      sessionStorage.setItem(draftKey(slug, challenge.id), JSON.stringify(next))
    } catch {
      /* the draft is only a convenience */
    }
  }
  const run = useMutation({
    mutationFn: () => platform.runCode(slug, challenge.id, language, source),
    onSuccess: (r) => {
      if (r.correct) {
        void client.invalidateQueries({ queryKey: ['challenges', slug] })
        void client.invalidateQueries({ queryKey: ['scoreboard', slug] })
        void client.invalidateQueries({ queryKey: ['progress', slug] })
      }
    },
  })
  if (!code) return null
  const locked = !open || challenge.solved
  const canRun = !locked && source.trim() !== '' && !run.isPending
  // Tab writes four spaces instead of leaving the box; Ctrl or Cmd with Enter runs.
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Tab' && !e.shiftKey) {
      e.preventDefault()
      const el = e.currentTarget
      const next = `${source.slice(0, el.selectionStart)}    ${source.slice(el.selectionEnd)}`
      const at = el.selectionStart + 4
      setSource(next)
      keep({ language, source: next })
      requestAnimationFrame(() => el.setSelectionRange(at, at))
    } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && canRun) {
      e.preventDefault()
      run.mutate()
    }
  }
  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-muted">
        {code.tests} {code.tests === 1 ? 'test' : 'tests'} · {code.time_limit_ms / 1000}s and {code.memory_mb} MB each. Read from the standard input, write to the standard output.
      </p>
      {code.samples.length > 0 && (
        <div className="grid gap-2 sm:grid-cols-2">
          {code.samples.map((s, i) => (
            <div key={i} className="min-w-0 rounded-control border border-line p-2.5 text-xs">
              <p className="mb-1 text-muted">Example {i + 1}</p>
              <pre className="whitespace-pre-wrap break-words font-mono">{s.input || ' '}</pre>
              <p className="mb-1 mt-2 text-muted">Output</p>
              <pre className="whitespace-pre-wrap break-words font-mono">{s.expected || ' '}</pre>
            </div>
          ))}
        </div>
      )}
      <div className="flex items-center gap-2">
        <Select
          value={language}
          aria-label="Language"
          onChange={(e) => {
            setLanguage(e.target.value)
            keep({ language: e.target.value, source })
          }}
          className="w-48"
          disabled={locked}
        >
          {code.languages.map((l) => (
            <option key={l.id} value={l.id}>
              {l.label}
            </option>
          ))}
        </Select>
        <span className="ml-auto text-xs text-muted">Ctrl+Enter runs</span>
      </div>
      <textarea
        value={source}
        onChange={(e) => {
          setSource(e.target.value)
          keep({ language, source: e.target.value })
        }}
        onKeyDown={onKey}
        aria-label="Your code"
        placeholder="Write your solution here"
        disabled={locked}
        spellCheck={false}
        autoCapitalize="off"
        autoComplete="off"
        rows={12}
        className="min-h-48 w-full resize-y rounded-control border border-field-line bg-field p-3 font-mono text-sm leading-relaxed outline-none focus:border-accent disabled:opacity-60"
      />
      <div className="flex items-center gap-3">
        <Button variant="primary" onClick={() => run.mutate()} disabled={!canRun}>
          <Play size={14} aria-hidden="true" /> {run.isPending ? 'Running...' : 'Run'}
        </Button>
        {!run.data && challenge.attempts_remaining !== null && challenge.max_attempts !== null && (
          <span className="text-sm text-muted">{challenge.attempts_remaining} attempts left</span>
        )}
        {run.data?.attempts_remaining !== undefined && run.data.attempts_remaining !== null && !run.data.correct && <span className="text-sm text-muted">{run.data.attempts_remaining} attempts left</span>}
        {challenge.solved && <span className="text-sm text-win">Solved</span>}
        {!open && !challenge.solved && <span className="text-sm text-muted">Closed</span>}
      </div>
      <ErrorText error={run.error} />
      {run.data && <CodeResult result={run.data} />}
    </div>
  )
}
