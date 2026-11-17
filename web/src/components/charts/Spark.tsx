/** A tiny line of recent activity, with no axes: enough to see whether something is rising. */
export function Spark({ values, label, className = '' }: { values: number[]; label: string; className?: string }) {
  const W = 80
  const H = 20
  const max = Math.max(1, ...values)
  const points = values.map((v, i) => `${values.length <= 1 ? 0 : (i / (values.length - 1)) * W},${H - 1 - (v / max) * (H - 3)}`).join(' ')
  return (
    <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} role="img" aria-label={label} className={`shrink-0 text-accent ${className}`}>
      <polyline points={points} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" strokeLinecap="round" />
    </svg>
  )
}
