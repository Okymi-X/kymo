import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { platform } from '../../api/platform'
import { Button, ErrorText, Field, Input, Textarea } from '../ui'
import { Panel } from './bits'
import { RulesText } from './RulesDialog'

/** Where the organizer writes the event's rules. Players read them in a dialog on the event page. */
export function RulesEditor({ eventId, slug }: { eventId: number; slug: string }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['rules', slug], queryFn: () => platform.eventRules(slug) })
  const [title, setTitle] = useState('Rules')
  const [body, setBody] = useState('')
  const [loaded, setLoaded] = useState(false)
  useEffect(() => {
    if (q.data && !loaded) {
      setTitle(q.data.title)
      setBody(q.data.body)
      setLoaded(true)
    }
  }, [q.data, loaded])
  const save = useMutation({
    mutationFn: () => platform.saveEventRules(eventId, { title, body }),
    onSuccess: (rules) => {
      setTitle(rules.title)
      setBody(rules.body)
      void client.invalidateQueries({ queryKey: ['rules', slug] })
    },
  })
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-lg font-semibold tracking-tight">Rules</h2>
      <p className="text-sm text-muted">Players open these from a button on the event page. Leave the text empty for no rules. A line starting with # is a heading, a line starting with - is a list item, and a blank line starts a new paragraph.</p>
      <Field label="Button and dialog title">
        <Input value={title} onChange={(e) => setTitle(e.target.value)} maxLength={60} className="max-w-xs" />
      </Field>
      <Field label="Rules">
        <Textarea value={body} onChange={(e) => setBody(e.target.value)} maxLength={20000} rows={10} className="font-mono text-sm" />
      </Field>
      {body.trim() !== '' && (
        <details className="rounded-control border border-line p-3">
          <summary className="cursor-pointer text-sm font-medium">Preview</summary>
          <div className="mt-3">
            <RulesText text={body} />
          </div>
        </details>
      )}
      <ErrorText error={save.error} />
      <div className="flex items-center gap-3">
        <Button variant="primary" onClick={() => save.mutate()} disabled={save.isPending}>
          Save rules
        </Button>
        {save.isSuccess && <span className="text-sm text-win">Saved</span>}
      </div>
    </Panel>
  )
}
