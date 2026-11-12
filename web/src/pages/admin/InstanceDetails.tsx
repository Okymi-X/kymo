import { useQuery } from '@tanstack/react-query'
import { platform } from '../../api/platform'
import { Modal, ModalBody, ModalHeader } from '../../components/Modal'
import { CopyButton } from '../../components/platform/bits'
import { ErrorText } from '../../components/ui'
import { timeAgo } from '../../lib/time'

const stateTone = { fixed: 'text-win', failed: 'text-danger', broken: 'text-muted' } as const

/** What an organizer can check about one run: the flags generated for it and what its Verifier recorded. */
export function InstanceDetails({ run, onClose }: { run: number; onClose: () => void }) {
  const flags = useQuery({ queryKey: ['admin-instance-flags', run], queryFn: () => platform.adminInstanceFlags(run), retry: false })
  const log = useQuery({ queryKey: ['admin-verify-log', run], queryFn: () => platform.adminVerifyLog(run), retry: false })
  return (
    <Modal label={`Instance ${run}`} onClose={onClose} width="40rem">
      <ModalHeader title={`Instance #${run}`} subtitle="Organizers only. Reading the flags is recorded in the audit log." />
      <ModalBody>
        <section className="flex flex-col gap-2" aria-label="Flags of this run">
          <h3 className="text-sm font-semibold">Flags of this run</h3>
          {flags.isPending && <p className="text-sm text-muted">Loading...</p>}
          {flags.data && flags.data.flags.length === 0 && <p className="text-sm text-muted">This run has no generated flags.</p>}
          {flags.data?.flags.map((f) => (
            <div key={f.name} className="flex flex-wrap items-center gap-2">
              <span className="w-24 text-sm text-muted">{f.label || f.name}</span>
              <code className="min-w-0 flex-1 break-all rounded-control border border-line bg-bg px-2 py-1 font-mono text-xs">{f.value}</code>
              <CopyButton text={f.value} />
            </div>
          ))}
          <ErrorText error={flags.error} />
        </section>
        <section className="flex flex-col gap-2" aria-label="Verifier log">
          <h3 className="text-sm font-semibold">Verifier runs</h3>
          {log.isPending && <p className="text-sm text-muted">Loading...</p>}
          {log.error && <p className="text-sm text-muted">This run has no Verifier, or it does not answer.</p>}
          {log.data && log.data.log.length === 0 && <p className="text-sm text-muted">The player has not pressed Verify yet.</p>}
          {log.data?.log
            .slice()
            .reverse()
            .map((entry, i) => (
              <div key={i} className="rounded-control border border-line p-2.5 text-sm">
                <p className="text-xs text-muted">
                  {timeAgo(entry.at)} · {entry.millis} ms
                </p>
                <ul className="mt-1 flex flex-col gap-0.5">
                  {entry.checks.map((c) => (
                    <li key={c.id} className="flex flex-wrap gap-2">
                      <span className="font-mono text-xs">{c.id}</span>
                      <span className={`text-xs font-medium ${stateTone[c.state]}`}>{c.state}</span>
                      {c.detail && <span className="text-xs text-muted">{c.detail}</span>}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
        </section>
      </ModalBody>
    </Modal>
  )
}
