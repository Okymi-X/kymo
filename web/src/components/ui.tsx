import { Search } from 'lucide-react'
import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from 'react'
import type { Status } from '../api/types'

const control =
  'h-10 rounded-control border border-field-line bg-field px-3 text-ink placeholder:text-muted/60 transition-[border-color,box-shadow] hover:border-muted focus:border-accent focus:outline-none focus:ring-[3px] focus:ring-accent/25 disabled:opacity-50'

type Variant = 'primary' | 'secondary' | 'danger' | 'ghost'

const variants: Record<Variant, string> = {
  primary: 'bg-accent text-bg border-transparent hover:opacity-90 disabled:bg-line disabled:text-muted disabled:opacity-100',
  secondary: 'bg-surface text-ink border-line hover:bg-line/40',
  danger: 'bg-surface text-danger border-line hover:bg-line/40',
  ghost: 'bg-transparent text-muted border-transparent hover:text-ink',
}

export function Button({
  variant = 'secondary',
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      {...props}
      className={`inline-flex h-10 items-center justify-center gap-2 rounded-control border px-3.5 font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${className}`}
    />
  )
}

export function Input({ className = '', ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={`${control} ${className}`} />
}

export function Textarea({ className = '', ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea {...props} className={`${control.replace('h-10 ', '')} min-h-28 py-2 ${className}`} />
}

export function SearchField({ className = '', label, ...props }: InputHTMLAttributes<HTMLInputElement> & { label: string }) {
  return (
    <div className={`relative ${className}`}>
      <Search size={15} aria-hidden="true" className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted" />
      <input type="search" aria-label={label} {...props} className={`${control} w-full pl-9`} />
    </div>
  )
}

export function Select({ className = '', ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={`${control} ${className}`} />
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-ink/70">{label}</span>
      {children}
    </label>
  )
}

const statusStyle: Record<Status, string> = {
  draft: 'text-muted',
  live: 'text-accent',
  done: 'text-win',
}

const statusLabel: Record<Status, string> = { draft: 'Draft', live: 'Live', done: 'Finished' }

export function StatusPill({ status }: { status: Status }) {
  return (
    <span className={`rounded-full border border-line px-2.5 py-0.5 text-xs font-medium ${statusStyle[status]}`}>
      {statusLabel[status]}
    </span>
  )
}

export function ErrorText({ error }: { error: unknown }) {
  if (!error) return null
  return (
    <p role="alert" className="text-sm text-danger">
      {error instanceof Error ? error.message : 'Something went wrong'}
    </p>
  )
}

export function PageTitle({ children }: { children: ReactNode }) {
  return <h1 className="text-4xl font-semibold leading-[1.05] tracking-[-0.03em] sm:text-5xl">{children}</h1>
}

interface SegmentedProps<T extends string> {
  label: string
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
}

/** A single choice among a few options, shown as one pill with a sliding highlight. */
export function Segmented<T extends string>({ label, value, options, onChange }: SegmentedProps<T>) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex gap-0.5 self-start rounded-control border border-field-line bg-field p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={`h-9 rounded-[calc(var(--radius-control)-2px)] px-3.5 text-sm font-medium transition-[background-color,color,box-shadow] duration-150 ${value === o.value ? 'bg-ink text-bg shadow-[var(--shadow-card)]' : 'text-muted hover:text-ink'}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}
