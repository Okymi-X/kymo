import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import { shortDuration } from '../../lib/platform'
import { timeAgo } from '../../lib/time'
import { ConfirmDialog } from '../ConfirmDialog'
import { Modal, ModalBody, ModalFooter, ModalHeader } from '../Modal'
import { DataTable, Loading, Pill, TD } from '../platform/bits'
import { Button, ErrorText, Field, Input, Select } from '../ui'
import { TimePlot } from '../charts/TimePlot'
import { BAN_LENGTHS, severityLabel, severityTone } from './constants'

function when(t: number): string {
  return new Date(t * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'medium' })
}

/** One alert: what was seen, the requests behind it, and what to do about it. */
export function AlertDialog({ id, canEdit, onClose }: { id: number; canEdit: boolean; onClose: () => void }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['monitor-alert', id], queryFn: () => platform.alertDetails(id) })
  const [length, setLength] = useState('900')
  const [note, setNote] = useState('')
  const [confirmIp, setConfirmIp] = useState(false)
  const done = () => {
    void client.invalidateQueries({ queryKey: ['monitor-alerts'] })
    void client.invalidateQueries({ queryKey: ['monitor-alert', id] })
    void client.invalidateQueries({ queryKey: ['monitor-series'] })
    void client.invalidateQueries({ queryKey: ['bans'] })
  }
  const secs = length === 'forever' ? undefined : Number(length)
  const ban = useMutation({
    mutationFn: (kind: 'user' | 'ip') => platform.banFromAlert(id, { kind, ...(secs ? { duration_secs: secs } : {}) }),
    onSuccess: () => {
      done()
      onClose()
    },
  })
  const dismiss = useMutation({
    mutationFn: () => platform.dismissAlert(id, note),
    onSuccess: () => {
      done()
      onClose()
    },
  })
  const d = q.data
  const a = d?.alert
  const subject = a ? (a.user_id !== null ? (a.username ?? `#${a.user_id}`) : `Visitor ${a.ip}`) : ''
  const trendTimes = d ? d.trend.map(([t]) => t) : []
  return (
    <Modal label="Alert" onClose={onClose} width="52rem">
      <ModalHeader title={a ? `${a.label}: ${subject}` : 'Alert'} subtitle={a ? `${when(a.first_at)} to ${when(a.last_at)}` : undefined} />
      <ModalBody>
        {q.isPending && <Loading />}
        <ErrorText error={q.error} />
        {a && d && (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <Pill tone={severityTone[a.severity]}>{severityLabel[a.severity]}</Pill>
              <Pill>{a.status === 'open' ? 'Open' : a.status === 'banned' ? 'Banned' : 'Dismissed'}</Pill>
              <span className="text-sm">
                <span className="font-mono tabular-nums">{a.count}</span> counted. Last seen {timeAgo(a.last_at)}
                {a.user_id !== null && (
                  <>
                    {' '}
                    from <span className="font-mono text-xs">{a.ip}</span>
                  </>
                )}
                .
              </span>
            </div>
            {a.status !== 'open' && (a.note || a.ban_id) && (
              <p className="text-sm text-muted">
                {a.status === 'banned' ? 'Banned' : 'Dismissed'}
                {a.handled_at ? ` ${timeAgo(a.handled_at)}` : ''}
                {a.note ? `: ${a.note}` : ''}
              </p>
            )}
            <section className="flex flex-col gap-2">
              <h3 className="text-sm font-semibold">What was seen</h3>
              <ul className="flex flex-col gap-1 rounded-control border border-line bg-bg p-3 font-mono text-xs">
                {a.evidence.length === 0 ? <li className="text-muted">No examples kept.</li> : a.evidence.map((e) => <li key={e} className="break-all">{e}</li>)}
              </ul>
            </section>
            {d.trend.length > 0 && (
              <section className="flex flex-col gap-2">
                <h3 className="text-sm font-semibold">Requests per minute around it</h3>
                <TimePlot
                  label="Requests per minute from this player around the alert"
                  times={trendTimes}
                  step={60}
                  top={Math.max(1, ...d.trend.map(([, n]) => n))}
                  tooltip={(i) => (
                    <span>
                      {new Date(d.trend[i][0] * 1000).toLocaleTimeString()}: <span className="font-mono">{d.trend[i][1]}</span>
                    </span>
                  )}
                  marks={(c) => d.trend.map(([t, n], i) => <rect key={t} x={c.x(i) - Math.max(1, c.cell * 0.4)} y={c.y(n)} width={Math.max(2, c.cell * 0.8)} height={Math.max(0.5, c.bottom - c.y(n))} className="fill-accent" />)}
                />
              </section>
            )}
            <section className="flex flex-col gap-2">
              <h3 className="text-sm font-semibold">
                Requests <span className="font-normal text-muted">({d.request_total}, newest {d.requests.length})</span>
              </h3>
              <DataTable caption="Requests behind the alert" head={['When', 'Request', 'Status', 'Browser']}>
                {d.requests.map((r) => (
                  <tr key={r.id}>
                    <td className={`${TD} whitespace-nowrap text-muted`}>{new Date(r.at * 1000).toLocaleTimeString()}</td>
                    <td className={`${TD} break-all font-mono text-xs`}>
                      {r.method} {r.path}
                      {r.query ? `?${r.query}` : ''}
                    </td>
                    <td className={`${TD} font-mono`}>{r.status}</td>
                    <td className={`${TD} max-w-48 truncate text-xs text-muted`} title={r.ua}>
                      {r.ua || 'none'}
                    </td>
                  </tr>
                ))}
              </DataTable>
            </section>
            {canEdit && a.status === 'open' && (
              <section className="flex flex-col gap-3 rounded-card border border-line p-4">
                <h3 className="text-sm font-semibold">What to do</h3>
                <div className="flex flex-wrap items-end gap-3">
                  <Field label="Ban for">
                    <Select value={length} onChange={(e) => setLength(e.target.value)}>
                      {BAN_LENGTHS.map((l) => (
                        <option key={l.label} value={l.secs ?? 'forever'}>
                          {l.label}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  {a.user_id !== null && (
                    <Button variant="danger" onClick={() => ban.mutate('user')} disabled={ban.isPending}>
                      Ban {a.username ?? 'the player'}
                    </Button>
                  )}
                  <Button variant="danger" onClick={() => setConfirmIp(true)} disabled={ban.isPending}>
                    Ban the address
                  </Button>
                </div>
                <div className="flex flex-wrap items-end gap-3">
                  <Field label="Note (optional)">
                    <Input value={note} onChange={(e) => setNote(e.target.value)} maxLength={300} className="w-72" placeholder="Why it is fine" />
                  </Field>
                  <Button onClick={() => dismiss.mutate()} disabled={dismiss.isPending}>
                    Dismiss
                  </Button>
                </div>
                <p className="text-xs text-muted">Banning a player ends their session and blocks their account{secs ? ` for ${shortDuration(secs)}` : ' until you lift it'}. A dismissed alert is not raised again for what was already seen.</p>
                <ErrorText error={ban.error ?? dismiss.error} />
              </section>
            )}
          </>
        )}
      </ModalBody>
      <ModalFooter>
        <Button onClick={onClose}>Close</Button>
      </ModalFooter>
      {confirmIp && a && (
        <ConfirmDialog
          title="Ban this address?"
          message={`Everyone using ${a.ip} is blocked, including other players on the same network. Ban the player instead when you can.`}
          confirmLabel="Ban the address"
          danger
          pending={ban.isPending}
          onConfirm={() => ban.mutate('ip')}
          onClose={() => setConfirmIp(false)}
        />
      )}
    </Modal>
  )
}
