import { useQuery } from '@tanstack/react-query'
import { BookOpen } from 'lucide-react'
import { useState } from 'react'
import { platform } from '../../api/platform'
import { Button } from '../ui'
import { RulesDialog } from './RulesDialog'

/** Opens the event's rules in a dialog. Not shown for an event that has none. The label is the organizer's title. */
export function RulesButton({ slug }: { slug: string }) {
  const [open, setOpen] = useState(false)
  const q = useQuery({ queryKey: ['rules', slug], queryFn: () => platform.eventRules(slug), staleTime: 60_000, retry: false })
  if (!q.data || q.data.body.trim() === '') return null
  return (
    <>
      <Button onClick={() => setOpen(true)} className="h-8 px-3 text-sm">
        <BookOpen size={15} aria-hidden="true" /> {q.data.title}
      </Button>
      {open && <RulesDialog slug={slug} onClose={() => setOpen(false)} />}
    </>
  )
}
