import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useNavigate } from 'react-router'
import { api } from '../api/client'
import type { Tournament } from '../api/types'
import { isHex } from '../lib/theme'
import { ConfirmDialog } from './ConfirmDialog'
import { ExportButton } from './ExportButton'
import { Modal, ModalBody, ModalFooter, ModalHeader, modalForm } from './Modal'
import { Button, ErrorText, Field, Input, Textarea } from './ui'

type Pending = 'reset' | 'delete' | 'rename' | 'color' | 'duplicate' | null

/** The organizer's tools for one tournament: rename, color, duplicate, back up, reset, delete.
 *  They live on the event's admin page, not on the public tournament page. */
export function TournamentAdmin({ t }: { t: Tournament }) {
  const client = useQueryClient()
  const navigate = useNavigate()
  const [asking, setAsking] = useState<Pending>(null)
  const reset = useMutation({
    mutationFn: () => api.reset(t.id),
    onSuccess: (next) => {
      client.setQueryData(['tournament', next.slug], next)
      setAsking(null)
    },
  })
  const [name, setName] = useState(t.name)
  const [description, setDescription] = useState(t.description)
  const duplicate = useMutation({
    mutationFn: () =>
      api.importSetup({
        name: `${t.name} copy`.slice(0, 80),
        format: t.format,
        third_place: t.third_place,
        groups: t.groups,
        advance: t.advance,
        rounds: t.rounds,
        best_of: t.best_of,
        description: t.description,
        teams: t.teams.map((team) => ({ name: team.name, score: team.score })),
      }),
    onSuccess: (copy) => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      if (copy.event_id !== null) navigate(`/admin/events/${copy.event_id}?view=tournament`)
    },
  })
  const [color, setColor] = useState(t.accent ?? '#0a6fe0')
  const recolor = useMutation({
    mutationFn: (accent: string | null) => api.setAccent(t.id, accent),
    onSuccess: (next) => {
      client.setQueryData(['tournament', next.slug], next)
      setAsking(null)
    },
  })
  const rename = useMutation({
    mutationFn: () => api.rename(t.id, name.trim(), description),
    onSuccess: (next) => {
      client.setQueryData(['tournament', next.slug], next)
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      void client.invalidateQueries({ queryKey: ['admin-event'] })
      void client.invalidateQueries({ queryKey: ['admin-events'] })
      setAsking(null)
    },
  })
  const remove = useMutation({
    mutationFn: () => api.remove(t.id),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      navigate('/admin/events')
    },
  })
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button
        onClick={() => {
          setName(t.name)
          setDescription(t.description)
          setAsking('rename')
        }}
      >
        Rename
      </Button>
      <Button
        onClick={() => {
          setColor(t.accent ?? '#0a6fe0')
          setAsking('color')
        }}
      >
        Color
      </Button>
      <Button onClick={() => setAsking('duplicate')}>Duplicate</Button>
      <ExportButton filename={`${t.slug}-backup.json`} build={() => JSON.stringify(t, null, 2)} label="Backup JSON" type="application/json" />
      {t.status !== 'draft' && <Button onClick={() => setAsking('reset')}>Reset bracket</Button>}
      <Button variant="danger" onClick={() => setAsking('delete')}>
        Delete
      </Button>
      <ErrorText error={reset.error ?? remove.error ?? duplicate.error} />
      {asking === 'color' && (
        <Modal label="Tournament color" onClose={() => setAsking(null)} backdropCloses={false}>
          <form
            className={modalForm}
            onSubmit={(e) => {
              e.preventDefault()
              if (isHex(color)) recolor.mutate(color)
            }}
          >
            <ModalHeader title="Tournament color" subtitle="Tints buttons and highlights on this tournament, its stage screen and its embed." />
            <ModalBody>
              <div className="flex items-end gap-3">
                <input type="color" aria-label="Pick a color" value={isHex(color) ? color : '#0a6fe0'} onChange={(e) => setColor(e.target.value)} className="h-10 w-14 cursor-pointer rounded-control border border-field-line bg-field p-1" />
                <Field label="Hex color">
                  <Input value={color} onChange={(e) => setColor(e.target.value)} maxLength={7} className="w-32 font-mono" />
                </Field>
              </div>
              <ErrorText error={recolor.error} />
            </ModalBody>
            <ModalFooter
              note={
                <Button type="button" variant="ghost" onClick={() => recolor.mutate(null)} disabled={recolor.isPending || t.accent === null}>
                  Use site color
                </Button>
              }
            >
              <Button type="button" variant="ghost" onClick={() => setAsking(null)}>
                Cancel
              </Button>
              <Button type="submit" variant="primary" disabled={recolor.isPending || !isHex(color)}>
                Save color
              </Button>
            </ModalFooter>
          </form>
        </Modal>
      )}
      {asking === 'rename' && (
        <Modal label="Rename tournament" onClose={() => setAsking(null)} backdropCloses={false} dirty={name.trim() !== t.name || description !== (t.description ?? '')}>
          <form
            className={modalForm}
            onSubmit={(e) => {
              e.preventDefault()
              if (name.trim()) rename.mutate()
            }}
          >
            <ModalHeader title="Rename tournament" />
            <ModalBody>
              <Field label="Tournament name">
                <Input value={name} maxLength={80} onChange={(e) => setName(e.target.value)} autoFocus />
              </Field>
              <Field label="Description, shown under the title">
                <Textarea value={description} onChange={(e) => setDescription(e.target.value)} maxLength={600} rows={4} placeholder="Rules, venue, check in time" />
              </Field>
              <ErrorText error={rename.error} />
            </ModalBody>
            <ModalFooter>
              <Button type="button" variant="ghost" onClick={() => setAsking(null)}>
                Cancel
              </Button>
              <Button type="submit" variant="primary" disabled={rename.isPending || !name.trim()}>
                Save name
              </Button>
            </ModalFooter>
          </form>
        </Modal>
      )}
      {asking === 'duplicate' && (
        <ConfirmDialog
          title={`Duplicate ${t.name}?`}
          message={`A new draft called "${t.name} copy" is created with the same format, settings and teams, and opened. Results are not copied, and this one is not changed.`}
          confirmLabel="Create the copy"
          pending={duplicate.isPending}
          onConfirm={() => duplicate.mutate()}
          onClose={() => setAsking(null)}
        />
      )}
      {asking === 'reset' && (
        <ConfirmDialog
          title="Reset the bracket?"
          message="All results are removed. The roster stays so you can start again."
          confirmLabel="Reset bracket"
          danger
          pending={reset.isPending}
          onConfirm={() => reset.mutate()}
          onClose={() => setAsking(null)}
        />
      )}
      {asking === 'delete' && (
        <ConfirmDialog
          title={`Delete ${t.name}?`}
          message="The tournament, its teams and its bracket are removed. This cannot be undone."
          confirmLabel="Delete"
          danger
          pending={remove.isPending}
          onConfirm={() => remove.mutate()}
          onClose={() => setAsking(null)}
        />
      )}
    </div>
  )
}
