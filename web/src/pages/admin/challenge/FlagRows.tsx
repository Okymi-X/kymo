import { Field, Input } from '../../../components/ui'
import type { Delivery } from '../../../lib/delivery'
import { DynamicTemplates } from './DynamicTemplates'
import { exampleFlag, MAX_FLAGS, rowProblems, splitPoints, type FlagRowDraft } from '../../../lib/flags'
import { int, type Draft } from './draft'

/** The flags of a dynamic or multi flag challenge: one card each, with the share of the points a weight gives. */
export function FlagRows({ d, set, shown, locked, known, delivery, tournament = false }: { tournament?: boolean; d: Draft; set: <K extends keyof Draft>(key: K, value: Draft[K]) => void; shown?: string; locked: boolean; known: string[]; delivery: Delivery }) {
  const dynamic = d.flagMode === 'dynamic'
  const count = Math.max(1, Math.min(MAX_FLAGS, int(d.flagCount) || 1))
  const rows = d.rows.slice(0, count)
  const names = rows.map((r) => r.name)
  const several = count > 1
  const shares = splitPoints(int(d.points), rows.map((r) => int(r.weight) || 1))
  const update = (i: number, patch: Partial<FlagRowDraft>) => set('rows', d.rows.map((r, j) => (j === i ? { ...r, ...patch } : r)))
  return (
    <div className="flex flex-col gap-3">
      {known.length > 0 && (
        <datalist id="manifest-flags">
          {known.map((k) => (
            <option key={k} value={k} />
          ))}
        </datalist>
      )}
      {rows.map((r, i) => {
        const bad = shown === undefined ? {} : rowProblems(r, d.flagMode, d.prefix, d.suffix, d.caseSensitive, names)
        return (
          <div key={i} className="flex flex-col gap-3 rounded-control border border-line p-3" role="group" aria-label={`Flag ${i + 1}`}>
            <div className="grid gap-3 sm:grid-cols-[10rem_1fr_6rem]">
              <Field label="Name" error={bad.name} hint={delivery === 'none' ? 'Used in the token {{flag:name}}.' : 'Matches the lab manifest.'}>
                <Input value={r.name} onChange={(e) => update(i, { name: e.target.value })} disabled={locked} className="font-mono" autoComplete="off" spellCheck={false} list={known.length > 0 ? 'manifest-flags' : undefined} aria-invalid={bad.name ? true : undefined} />
              </Field>
              <Field label="Label" hint="Only you see it.">
                <Input value={r.label} onChange={(e) => update(i, { label: e.target.value })} placeholder={i === 0 ? 'User flag' : ''} maxLength={60} />
              </Field>
              {several && (
                <Field label={tournament ? 'Points' : 'Weight'} error={bad.weight} hint={tournament ? 'Default 1.' : Number.isFinite(shares[i]) && d.points !== '' ? `${shares[i]} pts` : undefined}>
                  <Input type="number" min={1} value={r.weight} onChange={(e) => update(i, { weight: e.target.value })} disabled={locked} className="font-mono" aria-invalid={bad.weight ? true : undefined} />
                </Field>
              )}
            </div>
            <div className={dynamic ? 'grid gap-3 sm:grid-cols-[1fr_7rem_8rem]' : ''}>
              <Field label={dynamic ? 'Flag prefix' : 'Flag'} error={bad.value} hint={dynamic ? 'The readable start, for example KYMO{box_.' : undefined}>
                <Input value={r.value} onChange={(e) => update(i, { value: e.target.value })} disabled={locked} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={bad.value ? true : undefined} />
              </Field>
              {dynamic && (
                <>
                  <Field label="Random length" error={bad.randomLen} hint="Hex characters.">
                    <Input type="number" min={4} max={32} value={r.randomLen} onChange={(e) => update(i, { randomLen: e.target.value })} disabled={locked} className="font-mono" aria-invalid={bad.randomLen ? true : undefined} />
                  </Field>
                  <Field label="Closing" error={bad.closing} hint="After the random part.">
                    <Input value={r.closing} onChange={(e) => update(i, { closing: e.target.value })} disabled={locked} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={bad.closing ? true : undefined} />
                  </Field>
                </>
              )}
            </div>
            {dynamic && r.value && (
              <p className="text-xs text-muted">
                A player gets, for example: <code className="font-mono text-ink">{exampleFlag(r.value, int(r.randomLen), r.closing)}</code>
              </p>
            )}
          </div>
        )
      })}
      {(dynamic || d.responder) && <DynamicTemplates delivery={delivery} responder={d.responder} names={names} />}
      {shown && <p className="text-sm text-danger">{shown}</p>}
    </div>
  )
}

