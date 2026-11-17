import { Check } from 'lucide-react'
import type { EventState } from '../../api/platformTypes'
import { STEPS, stepOf } from '../../lib/lifecycle'

/** Where an event is on its way from draft to archived. A paused event stays on Live, marked. */
export function StateStepper({ state }: { state: EventState }) {
  const current = stepOf(state)
  return (
    <ol aria-label="Event progress" className="flex items-center">
      {STEPS.map((step, i) => {
        const done = i < current
        const here = i === current
        return (
          <li key={step.state} aria-current={here ? 'step' : undefined} className={`flex items-center ${i === STEPS.length - 1 ? '' : 'flex-1'}`}>
            <span className="flex items-center gap-2">
              <span
                aria-hidden="true"
                className={`flex size-5 shrink-0 items-center justify-center rounded-full border text-[10px] ${
                  here ? (state === 'paused' ? 'border-danger bg-danger text-bg' : 'border-accent bg-accent text-bg') : done ? 'border-line bg-line/60 text-muted' : 'border-line text-transparent'
                }`}
              >
                {done ? <Check size={12} /> : here ? '' : ''}
              </span>
              <span className={`whitespace-nowrap text-xs font-medium ${here ? 'text-ink' : 'text-muted'} ${here ? '' : 'hidden sm:inline'}`}>
                {step.label}
                {here && state === 'paused' && <span className="ml-1.5 text-danger">paused</span>}
              </span>
            </span>
            {i < STEPS.length - 1 && <span aria-hidden="true" className={`mx-3 h-px flex-1 ${i < current ? 'bg-muted/50' : 'bg-line'}`} />}
          </li>
        )
      })}
    </ol>
  )
}
