import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import { timeAgo } from '../../lib/time'
import { Pager } from '../Pager'
import { DataTable, Empty, Loading, Pill, TD, Toggle } from '../platform/bits'
import { Button, ErrorText, Segmented } from '../ui'
import { severityLabel, severityTone } from './constants'
import { DetectorSettings } from './DetectorSettings'

type Status = 'open' | 'handled' | 'all'

/** What the detectors found, newest first, open ones on top. */
export function AlertsTab({ canEdit, onOpen }: { canEdit: boolean; onOpen: (id: number) => void }) {
  const [status, setStatus] = useState<Status>('open')
  const [playersOnly, setPlayersOnly] = useState(false)
  const [page, setPage] = useState(1)
  const [settings, setSettings] = useState(false)
  const q = useQuery({
    queryKey: ['monitor-alerts', status, playersOnly, page],
    queryFn: () => platform.alerts({ status, who: playersOnly ? 'players' : 'all', page, per_page: 25 }),
    refetchInterval: 10_000,
    placeholderData: keepPreviousData,
  })
  const pages = q.data ? Math.max(1, Math.ceil(q.data.total / q.data.per_page)) : 1
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-4">
        <Segmented<Status>
          label="Alerts shown"
          value={status}
          onChange={(v) => {
            setStatus(v)
            setPage(1)
          }}
          options={[
            { value: 'open', label: 'Open' },
            { value: 'handled', label: 'Handled' },
            { value: 'all', label: 'All' },
          ]}
        />
        <Toggle
          label="Signed in players only"
          checked={playersOnly}
          onChange={(v) => {
            setPlayersOnly(v)
            setPage(1)
          }}
        />
        <Button variant="ghost" onClick={() => setSettings((v) => !v)} aria-expanded={settings} className="ml-auto">
          {settings ? 'Hide detection settings' : 'Detection settings'}
        </Button>
      </div>
      {settings && <DetectorSettings canEdit={canEdit} />}
      <ErrorText error={q.error} />
      {q.isPending ? (
        <Loading />
      ) : q.data && q.data.alerts.length > 0 ? (
        <>
          <DataTable caption="Alerts" head={['Level', 'Who', 'What', 'Counted', 'Last seen', 'State']}>
            {q.data.alerts.map((a) => (
              <tr key={a.id} className="cursor-pointer hover:bg-line/30" onClick={() => onOpen(a.id)}>
                <td className={TD}>
                  <Pill tone={severityTone[a.severity]}>{severityLabel[a.severity]}</Pill>
                </td>
                <td className={TD}>
                  <button type="button" className="text-left font-medium hover:underline" onClick={() => onOpen(a.id)}>
                    {a.user_id !== null ? (a.username ?? `#${a.user_id}`) : 'Visitor'}
                  </button>
                  <span className="block font-mono text-xs text-muted">{a.ip}</span>
                </td>
                <td className={TD}>{a.label}</td>
                <td className={`${TD} font-mono tabular-nums`}>{a.count}</td>
                <td className={`${TD} whitespace-nowrap text-muted`} title={new Date(a.last_at * 1000).toLocaleString()}>
                  {timeAgo(a.last_at)}
                </td>
                <td className={TD}>{a.status === 'open' ? <Pill tone="text-danger">Open</Pill> : <Pill>{a.status === 'banned' ? 'Banned' : 'Dismissed'}</Pill>}</td>
              </tr>
            ))}
          </DataTable>
          <Pager page={page} pages={pages} from={(page - 1) * q.data.per_page + 1} to={Math.min(q.data.total, page * q.data.per_page)} total={q.data.total} onChange={setPage} noun="alerts" />
        </>
      ) : (
        <Empty>{status === 'open' ? 'No open alerts. Nothing looks like an attack right now.' : 'No alerts here.'}</Empty>
      )}
    </div>
  )
}
