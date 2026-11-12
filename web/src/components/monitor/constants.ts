/** How long a ban can last, as offered in the lists. */
export const BAN_LENGTHS: { secs: number | null; label: string }[] = [
  { secs: 300, label: '5 minutes' },
  { secs: 900, label: '15 minutes' },
  { secs: 3600, label: '1 hour' },
  { secs: 86400, label: '1 day' },
  { secs: null, label: 'Permanently' },
]

export const severityTone = { high: 'text-danger', medium: 'text-[var(--series-3)]', low: 'text-muted' } as const
export const severityLabel = { high: 'High', medium: 'Medium', low: 'Low' } as const
