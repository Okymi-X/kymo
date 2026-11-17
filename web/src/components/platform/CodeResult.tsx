import { Check, X } from 'lucide-react'
import type { CodeResult as Result } from '../../api/platformTypes'
import { VERDICT_LABEL, passed, summary } from '../../lib/code'

/** How a run went: one line, the compiler's words when it did not build, then a row per test. A
 *  test whose input and output were shared opens to show them. */
export function CodeResult({ result }: { result: Result }) {
  const good = result.verdict === 'correct'
  return (
    <div className="flex flex-col gap-2" aria-live="polite">
      <p className={`text-sm font-medium ${good ? 'text-win' : result.compile_error !== null ? 'text-danger' : ''}`}>{summary(result)}</p>
      {result.compile_error !== null && <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-control border border-line bg-bg p-3 font-mono text-xs text-danger">{result.compile_error}</pre>}
      {result.cases.length > 0 && (
        <ul className="flex flex-col divide-y divide-line rounded-control border border-line">
          {result.cases.map((c, i) => {
            const head = (
              <span className="flex w-full items-center gap-2 px-3 py-2 text-sm">
                {passed(c) ? <Check size={14} aria-hidden="true" className="text-win" /> : <X size={14} aria-hidden="true" className="text-danger" />}
                <span className="font-medium">Test {i + 1}</span>
                {c.sample && <span className="text-xs text-muted">sample</span>}
                <span className={`ml-auto ${passed(c) ? 'text-win' : 'text-danger'}`}>{VERDICT_LABEL[c.verdict]}</span>
                <span className="w-16 text-right font-mono text-xs text-muted">{c.verdict === 'skipped' ? '' : c.ms === 0 ? '< 0.5 s' : `${c.ms} ms`}</span>
              </span>
            )
            return (
              <li key={i}>
                {c.input === undefined ? (
                  head
                ) : (
                  <details>
                    <summary className="cursor-pointer list-none">{head}</summary>
                    <div className="grid gap-2 border-t border-line bg-bg p-3 text-xs sm:grid-cols-3">
                      {([['Input', c.input], ['Expected', c.expected], ['Your output', c.stdout]] as const).map(([label, text]) => (
                        <div key={label} className="min-w-0">
                          <p className="mb-1 text-muted">{label}</p>
                          <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-words rounded border border-line bg-surface p-2 font-mono">{text || ' '}</pre>
                        </div>
                      ))}
                      {c.stderr && (
                        <div className="min-w-0 sm:col-span-3">
                          <p className="mb-1 text-muted">Errors</p>
                          <pre className="max-h-24 overflow-auto whitespace-pre-wrap break-words rounded border border-line bg-surface p-2 font-mono text-danger">{c.stderr}</pre>
                        </div>
                      )}
                    </div>
                  </details>
                )}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
