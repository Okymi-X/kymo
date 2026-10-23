import { useRef, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core'
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical, X } from 'lucide-react'
import { api } from '../api/client'
import type { Seeding, Tournament } from '../api/types'
import { Button, ErrorText, Field, Select } from './ui'

interface Item {
  key: number
  name: string
}

function Row({ item, seed, onRemove }: { item: Item; seed: number; onRemove: () => void }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.key })
  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={`flex items-center gap-2 rounded-control border border-line bg-surface px-2 py-1.5 ${isDragging ? 'relative z-10 shadow-lg' : ''}`}
    >
      <button type="button" aria-label={`Reorder ${item.name}`} className="cursor-grab touch-none p-1 text-muted" {...attributes} {...listeners}>
        <GripVertical size={16} />
      </button>
      <span className="w-7 font-mono text-xs tabular-nums text-muted">{seed}</span>
      <span className="flex-1 truncate">{item.name}</span>
      <button type="button" aria-label={`Remove ${item.name}`} onClick={onRemove} className="p-1 text-muted hover:text-danger">
        <X size={16} />
      </button>
    </li>
  )
}

export function RosterEditor({ tournament }: { tournament: Tournament }) {
  const nextKey = useRef(tournament.teams.length)
  const [items, setItems] = useState<Item[]>(() => tournament.teams.map((t, key) => ({ key, name: t.name })))
  const [draft, setDraft] = useState('')
  const [seeding, setSeeding] = useState<Seeding>('manual')
  const client = useQueryClient()
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const minTeams = tournament.format === 'double_elimination' ? 3 : 2
  const names = items.map((i) => i.name)
  const publish = (t: Tournament) => client.setQueryData(['tournament', t.slug], t)

  const save = useMutation({ mutationFn: () => api.setTeams(tournament.id, names), onSuccess: publish })
  const start = useMutation({
    mutationFn: async () => {
      await api.setTeams(tournament.id, names)
      return api.start(tournament.id, seeding)
    },
    onSuccess: publish,
  })

  const add = () => {
    const known = new Set(names.map((n) => n.toLowerCase()))
    const fresh = draft
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line && !known.has(line.toLowerCase()))
      .filter((line, i, all) => all.findIndex((l) => l.toLowerCase() === line.toLowerCase()) === i)
    setItems((prev) => [...prev, ...fresh.map((name) => ({ key: nextKey.current++, name }))])
    setDraft('')
  }

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return
    setItems((prev) => {
      const from = prev.findIndex((i) => i.key === active.id)
      const to = prev.findIndex((i) => i.key === over.id)
      return arrayMove(prev, from, to)
    })
  }

  return (
    <div className="grid gap-8 md:grid-cols-[1fr_20rem]">
      <div className="flex flex-col gap-3">
        <h2 className="text-sm font-medium text-muted">Seeding, top to bottom</h2>
        {items.length === 0 && <p className="text-muted">No teams yet. Add some on the right.</p>}
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
          <SortableContext items={items.map((i) => i.key)} strategy={verticalListSortingStrategy}>
            <ul className="flex flex-col gap-1.5">
              {items.map((item, index) => (
                <Row key={item.key} item={item} seed={index + 1} onRemove={() => setItems((p) => p.filter((i) => i.key !== item.key))} />
              ))}
            </ul>
          </SortableContext>
        </DndContext>
      </div>
      <div className="flex flex-col gap-4">
        <Field label="Add teams, one per line">
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            rows={5}
            className="rounded-control border border-line bg-surface px-3 py-2 text-ink"
          />
        </Field>
        <Button onClick={add} disabled={!draft.trim()}>
          Add to roster
        </Button>
        <hr className="border-line" />
        <Field label="Seeding on start">
          <Select value={seeding} onChange={(e) => setSeeding(e.target.value as Seeding)}>
            <option value="manual">Keep this order</option>
            <option value="random">Shuffle</option>
          </Select>
        </Field>
        <div className="flex gap-2">
          <Button onClick={() => save.mutate()} disabled={save.isPending}>
            Save roster
          </Button>
          <Button variant="primary" className="flex-1" onClick={() => start.mutate()} disabled={items.length < minTeams || start.isPending}>
            Start tournament
          </Button>
        </div>
        {items.length < minTeams && <p className="text-sm text-muted">Needs at least {minTeams} teams to start.</p>}
        <ErrorText error={save.error ?? start.error} />
      </div>
    </div>
  )
}
