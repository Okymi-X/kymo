import { useRef, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type Announcements,
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
import { Avatar } from './Avatar'
import { Button, ErrorText, Field, Select, Textarea } from './ui'

interface Item {
  key: number
  name: string
  score: number
}

function Row({
  item,
  seed,
  onScore,
  onRemove,
}: {
  item: Item
  seed: number
  onScore: (score: number) => void
  onRemove: () => void
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.key })
  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={`flex items-center gap-2 rounded-control border border-line bg-surface px-2 py-1.5 ${isDragging ? 'relative z-10 shadow-lg' : ''}`}
    >
      <button type="button" aria-label={`Reorder ${item.name}`} className="-m-1 cursor-grab touch-none p-2.5 text-muted" {...attributes} {...listeners}>
        <GripVertical size={16} />
      </button>
      <span className="w-7 font-mono text-xs tabular-nums text-muted">{seed}</span>
      <Avatar name={item.name} size={24} />
      <span className="flex-1 truncate">{item.name}</span>
      <input
        type="number"
        min={0}
        inputMode="numeric"
        aria-label={`Qualifier score for ${item.name}`}
        value={item.score}
        onChange={(e) => onScore(Math.max(0, Number(e.target.value) || 0))}
        className="h-8 w-20 rounded-control border border-field-line bg-field px-2 text-right font-mono text-xs tabular-nums focus:border-accent focus:outline-none focus:ring-[3px] focus:ring-accent/25"
      />
      <button type="button" aria-label={`Remove ${item.name}`} onClick={onRemove} className="-m-1 p-2.5 text-muted hover:text-danger">
        <X size={16} />
      </button>
    </li>
  )
}

/** Accepts "Name" or "Name, 420" where the number is a qualifier score. */
function parseLine(line: string): Omit<Item, 'key'> | null {
  const trimmed = line.trim()
  if (!trimmed) return null
  const match = /^(.*\S)\s*,\s*(\d+)$/.exec(trimmed)
  return match ? { name: match[1], score: Number(match[2]) } : { name: trimmed, score: 0 }
}

export function RosterEditor({ tournament }: { tournament: Tournament }) {
  const nextKey = useRef(tournament.teams.length)
  const [items, setItems] = useState<Item[]>(() => tournament.teams.map((t, key) => ({ key, name: t.name, score: t.score })))
  const [draft, setDraft] = useState('')
  const [seeding, setSeeding] = useState<Seeding>('manual')
  const client = useQueryClient()
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const minTeams =
    tournament.format === 'single_elimination'
      ? tournament.third_place
        ? 4
        : 2
      : tournament.format === 'group_knockout'
        ? tournament.groups * 3
        : 3
  const names = items.map((i) => i.name)
  const payload = items.map(({ name, score }) => ({ name, score }))
  const publish = (t: Tournament) => client.setQueryData(['tournament', t.slug], t)

  const save = useMutation({ mutationFn: () => api.setTeams(tournament.id, payload), onSuccess: publish })
  const start = useMutation({
    mutationFn: async () => {
      await api.setTeams(tournament.id, payload)
      return api.start(tournament.id, seeding)
    },
    onSuccess: publish,
  })

  const add = () => {
    const known = new Set(names.map((n) => n.toLowerCase()))
    const parsed = draft.split('\n').map(parseLine).filter((t): t is Omit<Item, 'key'> => t !== null)
    const fresh = parsed.filter((t) => {
      const id = t.name.toLowerCase()
      if (known.has(id)) return false
      known.add(id)
      return true
    })
    setItems((prev) => [...prev, ...fresh.map((t) => ({ ...t, key: nextKey.current++ }))])
    setDraft('')
  }

  const nameOf = (id: string | number) => items.find((i) => i.key === id)?.name ?? 'Team'
  const seedOf = (id: string | number) => items.findIndex((i) => i.key === id) + 1
  const announcements: Announcements = {
    onDragStart: ({ active }) => `Picked up ${nameOf(active.id)}, seed ${seedOf(active.id)} of ${items.length}.`,
    onDragOver: ({ active, over }) => (over ? `${nameOf(active.id)} is over seed ${seedOf(over.id)}, ${nameOf(over.id)}.` : undefined),
    onDragEnd: ({ active, over }) => (over ? `${nameOf(active.id)} dropped at seed ${seedOf(over.id)}.` : `${nameOf(active.id)} dropped where it was.`),
    onDragCancel: ({ active }) => `Move cancelled. ${nameOf(active.id)} stays at seed ${seedOf(active.id)}.`,
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
        <div className="flex items-baseline justify-between">
          <h2 className="text-sm font-medium text-muted">Seeding, top to bottom</h2>
          <p className="text-xs text-muted">Qualifier score on the right</p>
        </div>
        {items.length === 0 && <p className="text-muted">No teams yet. Add some on the right.</p>}
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd} accessibility={{ announcements }}>
          <SortableContext items={items.map((i) => i.key)} strategy={verticalListSortingStrategy}>
            <ul className="flex flex-col gap-1.5">
              {items.map((item, index) => (
                <Row
                  key={item.key}
                  item={item}
                  seed={index + 1}
                  onScore={(score) => setItems((p) => p.map((i) => (i.key === item.key ? { ...i, score } : i)))}
                  onRemove={() => setItems((p) => p.filter((i) => i.key !== item.key))}
                />
              ))}
            </ul>
          </SortableContext>
        </DndContext>
      </div>
      <div className="flex flex-col gap-4">
        <Field label="Add teams, one per line (optional: Name, score)">
          <Textarea value={draft} onChange={(e) => setDraft(e.target.value)} rows={5} placeholder={'Alpha, 900\nBravo, 750'} />
        </Field>
        <Button onClick={add} disabled={!draft.trim()}>
          Add to roster
        </Button>
        <hr className="border-line" />
        <Field label="Seeding on start">
          <Select value={seeding} onChange={(e) => setSeeding(e.target.value as Seeding)}>
            <option value="manual">Keep this order</option>
            <option value="score">By qualifier score</option>
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
