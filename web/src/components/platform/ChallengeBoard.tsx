import { useQuery } from '@tanstack/react-query'
import { Check, Lock } from 'lucide-react'
import { useMemo, useState } from 'react'
import { platform } from '../../api/platform'
import { useEventStream } from '../../api/platformHooks'
import { groupByCategory } from '../../lib/platform'
import { ErrorText } from '../ui'
import { Empty, Loading } from './bits'
import { ChallengeDialog } from './ChallengeDialog'

export function ChallengeBoard({ slug, vpn = false, labHost = null }: { slug: string; vpn?: boolean; labHost?: number | null }) {
  useEventStream(slug, true)
  const q = useQuery({ queryKey: ['challenges', slug], queryFn: () => platform.challenges(slug), refetchInterval: 20_000 })
  const [openId, setOpenId] = useState<number | null>(null)
  const groups = useMemo(() => (q.data ? groupByCategory(q.data.categories, q.data.challenges) : []), [q.data])
  if (q.isPending) return <Loading label="Loading challenges" />
  // A failed refresh must not replace a board that is on screen: an open challenge would vanish with the flag being typed.
  if (q.error && !q.data) return <ErrorText error={q.error} />
  const selected = q.data?.challenges.find((c) => c.id === openId)
  const category = groups.find((g) => g.challenges.some((c) => c.id === openId))?.category.name ?? ''
  if (groups.length === 0) return <Empty>No challenges are visible yet.</Empty>
  return (
    <div className="flex flex-col gap-8">
      {groups.map(({ category: cat, challenges }) => (
        <section key={cat.id} aria-labelledby={`cat-${cat.id}`} className="flex flex-col gap-3">
          <h2 id={`cat-${cat.id}`} className="text-sm font-semibold uppercase tracking-wide text-muted">
            {cat.name}
          </h2>
          <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {challenges.map((c) => (
              <li key={c.id}>
                <button
                  type="button"
                  onClick={() => setOpenId(c.id)}
                  className={`flex h-full w-full flex-col gap-2 rounded-card border p-4 text-left transition-[transform,box-shadow] hover:shadow-[var(--shadow-lift)] active:scale-[0.99] ${c.solved ? 'border-win/50 bg-win/5' : 'border-line bg-surface'} ${c.status === 'closed' ? 'opacity-70' : ''}`}
                >
                  <span className="flex items-start justify-between gap-2">
                    <span className="font-medium">{c.title}</span>
                    {c.solved ? (
                      <Check size={18} aria-label="Solved" className="shrink-0 text-win" />
                    ) : c.status === 'closed' ? (
                      <Lock size={16} aria-label="Closed" className="shrink-0 text-muted" />
                    ) : null}
                  </span>
                  <span className="mt-auto flex items-center justify-between text-sm text-muted">
                    <span className="font-mono tabular-nums text-ink">{c.points} pts</span>
                    <span>
                      {c.kind === 'dfir'
                        ? `${c.items.filter((i) => i.solved).length} of ${c.items.length} answered`
                        : `${c.solved_by} ${c.solved_by === 1 ? 'solve' : 'solves'}`}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ))}
      {selected && <ChallengeDialog slug={slug} challenge={selected} category={category} vpn={vpn} labHost={labHost} onClose={() => setOpenId(null)} />}
    </div>
  )
}
