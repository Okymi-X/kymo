import { useMutation } from '@tanstack/react-query'
import { Paperclip, Trash2 } from 'lucide-react'
import { useRef, useState } from 'react'
import { platform } from '../../../api/platform'
import type { AdminChallenge } from '../../../api/platformTypes'
import { Button, ErrorText, Field, Input } from '../../../components/ui'
import { fileSize, verdictText } from '../../../lib/platform'
import { inRange, int } from './draft'
import { Section } from './parts'

export function Extras({ challenge, tournament, onChanged }: { challenge: AdminChallenge; tournament: boolean; onChanged: () => void }) {
  const [hint, setHint] = useState('')
  const [cost, setCost] = useState('10')
  const addHint = useMutation({
    mutationFn: () => platform.addHint(challenge.id, hint.trim(), int(cost)),
    onSuccess: () => {
      setHint('')
      onChanged()
    },
  })
  const delHint = useMutation({ mutationFn: (id: number) => platform.deleteHint(id), onSuccess: onChanged })
  const file = useRef<HTMLInputElement>(null)
  const [asTemplate, setAsTemplate] = useState(false)
  const upload = useMutation({ mutationFn: (f: File) => platform.uploadAttachment(challenge.id, f, asTemplate), onSuccess: onChanged })
  const [preview, setPreview] = useState<{ name: string; text: string } | null>(null)
  const show = useMutation({ mutationFn: async (a: { id: number; name: string }) => ({ name: a.name, text: await platform.previewAttachment(a.id) }), onSuccess: setPreview })
  const canTemplate = challenge.kind === 'flag' && challenge.flag_mode === 'dynamic'
  const delFile = useMutation({ mutationFn: (id: number) => platform.deleteAttachment(id), onSuccess: onChanged })
  const [probe, setProbe] = useState('')
  const test = useMutation({ mutationFn: () => platform.testFlag(challenge.id, probe) })
  const costOk = inRange(cost, 0, 100000)
  return (
    <>
      {!tournament && (
      <Section title="Hints" help="Players pay a hint's cost, deducted when they solve. Changes save at once.">
        {challenge.hints.length === 0 && <p className="text-sm text-muted">No hints yet.</p>}
        {challenge.hints.map((h) => (
          <div key={h.id} className="flex items-start gap-2 rounded-control border border-line p-2.5 text-sm">
            <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">{h.content}</span>
            <span className="font-mono text-xs text-muted">{h.cost} pts</span>
            <button type="button" aria-label="Delete hint" onClick={() => delHint.mutate(h.id)} className="text-muted hover:text-danger">
              <Trash2 size={15} aria-hidden="true" />
            </button>
          </div>
        ))}
        <div className="grid grid-cols-[1fr_6rem_auto] items-end gap-2 max-sm:grid-cols-1">
          <Field label="New hint">
            <Input value={hint} onChange={(e) => setHint(e.target.value)} placeholder="What should a stuck player try?" />
          </Field>
          <Field label="Cost">
            <Input type="number" min={0} value={cost} onChange={(e) => setCost(e.target.value)} aria-label="Hint cost" className="font-mono" />
          </Field>
          <Button onClick={() => addHint.mutate()} disabled={!hint.trim() || !costOk || addHint.isPending}>
            Add hint
          </Button>
        </div>
        <ErrorText error={addHint.error ?? delHint.error} />
      </Section>
      )}

      <Section title="Attachments" help="Files players download from the challenge, up to 10 MB each. Changes here save at once.">
        {challenge.attachments.length === 0 && <p className="text-sm text-muted">No files yet.</p>}
        {challenge.attachments.map((a) => (
          <div key={a.id} className="flex items-center gap-2 rounded-control border border-line p-2.5 text-sm">
            <Paperclip size={14} aria-hidden="true" className="text-muted" />
            <span className="min-w-0 flex-1 truncate">{a.name}</span>
            {a.template && <span className="rounded-full border border-accent/40 px-2 py-0.5 text-xs text-accent">per-player</span>}
            {a.template && (
              <button type="button" className="text-xs font-medium text-accent hover:underline" onClick={() => show.mutate({ id: a.id, name: a.name })}>
                Preview
              </button>
            )}
            <span className="font-mono text-xs text-muted">{fileSize(a.size)}</span>
            <button type="button" aria-label={`Delete ${a.name}`} onClick={() => delFile.mutate(a.id)} className="text-muted hover:text-danger">
              <Trash2 size={15} aria-hidden="true" />
            </button>
          </div>
        ))}
        <input
          ref={file}
          type="file"
          className="sr-only"
          aria-label="Attachment file"
          onChange={(e) => {
            const f = e.target.files?.[0]
            if (f) upload.mutate(f)
            e.target.value = ''
          }}
        />
        {canTemplate && (
          <label className="flex items-start gap-2 text-sm">
            <input type="checkbox" checked={asTemplate} onChange={(e) => setAsTemplate(e.target.checked)} className="mt-1" />
            <span>
              Per-player template: a text file with <code className="font-mono">{'{{flag}}'}</code> or <code className="font-mono">{'{{flag:name}}'}</code>. Each player downloads their own copy with their own flag.
            </span>
          </label>
        )}
        <Button className="self-start" onClick={() => file.current?.click()} disabled={upload.isPending}>
          {upload.isPending ? 'Uploading...' : asTemplate ? 'Upload a template' : 'Upload a file'}
        </Button>
        {preview && (
          <div className="rounded-control border border-line bg-bg p-3">
            <p className="mb-1 text-xs text-muted">What a player gets from {preview.name}, with made-up flags:</p>
            <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words font-mono text-xs">{preview.text}</pre>
            <Button variant="ghost" className="mt-2" onClick={() => setPreview(null)}>Close preview</Button>
          </div>
        )}
        <ErrorText error={upload.error ?? delFile.error ?? show.error} />
      </Section>

      {challenge.kind === 'flag' && (challenge.flag_mode === 'dynamic' || challenge.multi_flag) && (
      <Section title="Test the flag" help="Dynamic flags are per player. Preview a template above, or read a run's flag on Instances.">
        <p className="text-sm text-muted">Not available for this challenge.</p>
      </Section>
      )}
      {challenge.kind === 'flag' && challenge.flag_mode !== 'dynamic' && !challenge.multi_flag && (
      <Section title="Test the flag" help="Checks a flag the way a player's submission is checked, without recording a submission or a solve.">
        <div className="flex gap-2">
          <Input value={probe} onChange={(e) => setProbe(e.target.value)} placeholder="Try a flag" aria-label="Flag to test" className="min-w-0 flex-1 font-mono" autoComplete="off" />
          <Button onClick={() => test.mutate()} disabled={!probe || test.isPending}>
            Test
          </Button>
        </div>
        {test.data && <p className={test.data.verdict === 'correct' ? 'text-sm text-win' : 'text-sm text-danger'}>{verdictText[test.data.verdict]}</p>}
        <ErrorText error={test.error} />
      </Section>
      )}
    </>
  )
}

