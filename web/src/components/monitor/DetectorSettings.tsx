import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { Detector, DetectorConfig } from '../../api/platformTypes'
import { shortDuration } from '../../lib/platform'
import { Loading, Panel, Pill, Toggle } from '../platform/bits'
import { Button, ErrorText, Field, Input, Select } from '../ui'
import { BAN_LENGTHS } from './constants'

/** The text of one detector's rule, so the numbers read as a sentence. */
function sentence(c: DetectorConfig): string {
  if (!c.enabled) return 'Switched off.'
  const alert = `Alert at ${c.alert_at} within ${shortDuration(c.window_secs)}`
  if (c.ban_at === null) return `${alert}. Never bans by itself.`
  return `${alert}. At ${c.ban_at} the player is banned ${c.ban_secs === null ? 'permanently' : `for ${shortDuration(c.ban_secs)}`}.`
}

function toNumber(raw: string): number | null {
  const n = Number(raw)
  return raw.trim() !== '' && Number.isInteger(n) && n >= 0 ? n : null
}

function Card({ d, canEdit }: { d: Detector; canEdit: boolean }) {
  const client = useQueryClient()
  const [enabled, setEnabled] = useState(d.config.enabled)
  const [alertAt, setAlertAt] = useState(String(d.config.alert_at))
  const [banAt, setBanAt] = useState(d.config.ban_at === null ? '' : String(d.config.ban_at))
  const [window, setWindow] = useState(String(d.config.window_secs))
  const [banSecs, setBanSecs] = useState(d.config.ban_secs === null ? 'forever' : String(d.config.ban_secs))
  const next: DetectorConfig | null = (() => {
    const a = toNumber(alertAt)
    const w = toNumber(window)
    const b = banAt.trim() === '' ? null : toNumber(banAt)
    if (a === null || w === null || (banAt.trim() !== '' && b === null)) return null
    return { enabled, alert_at: a, ban_at: b, window_secs: w, ban_secs: banSecs === 'forever' ? null : Number(banSecs) }
  })()
  const problem =
    next === null ? 'Enter whole numbers.' : next.ban_at !== null && next.ban_at < next.alert_at ? 'The ban level must be at least the alert level.' : next.window_secs < 10 || next.window_secs > 3600 ? 'The window is 10 seconds to 1 hour.' : null
  const store = (r: { detectors: Detector[] }) => client.setQueryData(['monitor-detectors'], r)
  const save = useMutation({ mutationFn: () => platform.saveDetector(d.kind, next as DetectorConfig), onSuccess: store })
  const reset = useMutation({
    mutationFn: () => platform.resetDetector(d.kind),
    onSuccess: (r) => {
      store(r)
      const fresh = r.detectors.find((x) => x.kind === d.kind)
      if (fresh) {
        setEnabled(fresh.config.enabled)
        setAlertAt(String(fresh.config.alert_at))
        setBanAt(fresh.config.ban_at === null ? '' : String(fresh.config.ban_at))
        setWindow(String(fresh.config.window_secs))
        setBanSecs(fresh.config.ban_secs === null ? 'forever' : String(fresh.config.ban_secs))
      }
    },
  })
  const changed = next !== null && JSON.stringify(next) !== JSON.stringify(d.config)
  return (
    <Panel className="flex min-w-0 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h4 className="font-medium">{d.label}</h4>
        {d.customized && <Pill>Changed</Pill>}
        <span className="ml-auto">
          <Toggle label="On" checked={enabled} onChange={setEnabled} disabled={!canEdit} />
        </span>
      </div>
      <p className="text-sm text-muted">{d.description}</p>
      <div className="grid grid-cols-2 gap-3">
        <Field label="Alert at" hint={`Default ${d.default.alert_at}`}>
          <Input inputMode="numeric" value={alertAt} onChange={(e) => setAlertAt(e.target.value.replace(/\D/g, ''))} disabled={!canEdit} className="font-mono" />
        </Field>
        <Field label="Ban the player at" hint={d.default.ban_at === null ? 'Default never' : `Default ${d.default.ban_at}. Empty: never`}>
          <Input inputMode="numeric" value={banAt} onChange={(e) => setBanAt(e.target.value.replace(/\D/g, ''))} disabled={!canEdit} placeholder="never" className="font-mono" />
        </Field>
        <Field label="Count within (seconds)" hint={`Default ${d.default.window_secs}`}>
          <Input inputMode="numeric" value={window} onChange={(e) => setWindow(e.target.value.replace(/\D/g, ''))} disabled={!canEdit} className="font-mono" />
        </Field>
        <Field label="A ban lasts">
          <Select value={banSecs} onChange={(e) => setBanSecs(e.target.value)} disabled={!canEdit}>
            {BAN_LENGTHS.map((l) => (
              <option key={l.label} value={l.secs ?? 'forever'}>
                {l.label}
              </option>
            ))}
            {!BAN_LENGTHS.some((l) => String(l.secs) === banSecs) && banSecs !== 'forever' && <option value={banSecs}>{shortDuration(Number(banSecs))}</option>}
          </Select>
        </Field>
      </div>
      <p className="text-xs text-muted">{next ? sentence(next) : sentence(d.config)}</p>
      {canEdit && (
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="primary" onClick={() => save.mutate()} disabled={!changed || problem !== null || save.isPending}>
            Save
          </Button>
          {d.customized && (
            <Button variant="ghost" onClick={() => reset.mutate()} disabled={reset.isPending}>
              Restore default
            </Button>
          )}
          {problem && changed !== false && <span role="alert" className="text-sm text-danger">{problem}</span>}
        </div>
      )}
      <ErrorText error={save.error ?? reset.error} />
    </Panel>
  )
}

/** How each detector counts and when it alerts or bans. Tighten or loosen as the event needs. */
export function DetectorSettings({ canEdit }: { canEdit: boolean }) {
  const q = useQuery({ queryKey: ['monitor-detectors'], queryFn: platform.detectors })
  if (q.isPending) return <Loading />
  return (
    <div className="flex flex-col gap-3">
      <p className="max-w-3xl text-sm text-muted">
        Each detector counts one kind of behaviour. Reaching the alert level raises an alert for you to look at. Reaching the ban level bans a signed in player automatically, for the time set here. A visitor known only by address is never banned automatically, because an address can be a whole network. Staff are never counted.
      </p>
      <ErrorText error={q.error} />
      <div className="grid gap-4 lg:grid-cols-2">
        {(q.data?.detectors ?? []).map((d) => (
          <Card key={`${d.kind}-${JSON.stringify(d.config)}`} d={d} canEdit={canEdit} />
        ))}
      </div>
    </div>
  )
}
