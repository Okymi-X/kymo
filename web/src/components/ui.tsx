import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from 'react'
import type { Status } from '../api/types'

const control =
  'rounded-control border border-line bg-surface px-3 py-2 text-ink placeholder:text-muted disabled:opacity-50'

type Variant = 'primary' | 'secondary' | 'danger' | 'ghost'

const variants: Record<Variant, string> = {
  primary: 'bg-accent text-bg border-transparent hover:opacity-90',
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
      className={`inline-flex items-center justify-center gap-2 rounded-control border px-3.5 py-2 font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${className}`}
    />
  )
}

export function Input({ className = '', ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={`${control} ${className}`} />
}

export function Select({ className = '', ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={`${control} ${className}`} />
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-muted">{label}</span>
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
