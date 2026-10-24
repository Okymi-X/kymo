import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useParams } from 'react-router'
import { api } from '../api/client'
import { Button, ErrorText, Field, Input, PageTitle } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

/** A team captain's private page: report the score of the next match. The organizer approves it. */
export function ReportPage() {
  usePageTitle('Report a score')
  const { code = '' } = useParams()
  const client = useQueryClient()
  const info = useQuery({ queryKey: ['report', code], queryFn: () => api.reportInfo(code) })
  const [own, setOwn] = useState('')
  const [other, setOther] = useState('')
  const [sent, setSent] = useState(false)
  const submit = useMutation({
    mutationFn: () => api.submitReport(code, Number(own), Number(other)),
    onSuccess: () => {
      setSent(true)
      void client.invalidateQueries({ queryKey: ['report', code] })
    },
  })
  const send = (e: FormEvent) => {
    e.preventDefault()
    if (own !== '' && other !== '') submit.mutate()
  }

  if (info.isPending) return <p className="text-muted">Loading</p>
  if (info.error || !info.data) {
    return (
      <div className="mx-auto flex max-w-sm flex-col gap-3">
        <PageTitle>Link not valid</PageTitle>
        <p className="text-muted">This reporting link is wrong or was replaced. Ask the organizer for a new one.</p>
      </div>
    )
  }
  const d = info.data
  return (
    <div className="mx-auto flex max-w-sm flex-col gap-5">
      <div>
        <p className="text-sm text-muted">{d.tournament}</p>
        <PageTitle>{d.team}</PageTitle>
      </div>
      {d.match_id === null ? (
        <p className="text-muted">There is no match to report right now. Check the bracket for what is next.</p>
      ) : (
        <form onSubmit={send} className="flex flex-col gap-4">
          <p>
            Round {d.round}, against <strong>{d.opponent}</strong>.
          </p>
          <div className="grid grid-cols-2 gap-3">
            <Field label={`${d.team} score`}>
              <Input type="number" min={0} inputMode="numeric" value={own} onChange={(e) => setOwn(e.target.value)} className="font-mono" autoFocus />
            </Field>
            <Field label={`${d.opponent} score`}>
              <Input type="number" min={0} inputMode="numeric" value={other} onChange={(e) => setOther(e.target.value)} className="font-mono" />
            </Field>
          </div>
          {d.reported && !sent && (
            <p className="text-sm text-muted">
              You already reported {d.reported[0]}:{d.reported[1]}. Submitting again replaces it.
            </p>
          )}
          {sent && (
            <p role="status" className="rounded-control border border-line px-3 py-2 text-sm">
              Sent. The organizer will check it and confirm the result.
            </p>
          )}
          <ErrorText error={submit.error} />
          <Button type="submit" variant="primary" disabled={own === '' || other === '' || submit.isPending}>
            Send score
          </Button>
        </form>
      )}
      <Link to={`/t/${d.slug}`} className="text-sm font-medium text-accent hover:underline">
        View the tournament
      </Link>
    </div>
  )
}
