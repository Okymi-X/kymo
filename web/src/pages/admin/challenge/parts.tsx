import type { ReactNode } from 'react'
import { Button, Field, Input } from '../../../components/ui'

export function Section({ title, help, children }: { title: string; help?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-4 border-t border-line pt-5 first:border-t-0 first:pt-0">
      <div>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-muted">{title}</h3>
        {help && <p className="mt-1 text-xs text-muted">{help}</p>}
      </div>
      {children}
    </section>
  )
}

export function Num({ label, value, onChange, error, hint, min = 0 }: { label: string; value: string; onChange: (v: string) => void; error?: string; hint?: ReactNode; min?: number }) {
  return (
    <Field label={label} error={error} hint={hint}>
      <Input type="number" min={min} value={value} onChange={(e) => onChange(e.target.value)} aria-invalid={error ? true : undefined} className="font-mono" />
    </Field>
  )
}

/** A date and time with a clear button, so an optional limit can be taken away again. */
export function When({ label, value, onChange, error, hint }: { label: string; value: string; onChange: (v: string) => void; error?: string; hint?: ReactNode }) {
  return (
    <Field label={label} error={error} hint={hint}>
      <span className="flex gap-2">
        <Input type="datetime-local" value={value} onChange={(e) => onChange(e.target.value)} aria-invalid={error ? true : undefined} className="min-w-0 flex-1" />
        {value && (
          <Button variant="ghost" onClick={() => onChange('')} aria-label={`Clear ${label.toLowerCase()}`}>
            Clear
          </Button>
        )}
      </span>
    </Field>
  )
}
