import { useEffect, useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Navigate } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import type { Appearance, FontChoice } from '../api/types'
import { appearanceKey, useAppearance } from '../components/ThemeLoader'
import { Button, ErrorText, Field, Input, Select, PageTitle } from '../components/ui'
import {
  AA,
  DEFAULT_APPEARANCE,
  FONT_LABELS,
  PAGE_BACKGROUND,
  applyAppearance,
  contrastRatio,
  isHex,
} from '../lib/theme'

function ColorField({ label, value, onChange, against }: { label: string; value: string; onChange: (v: string) => void; against: string }) {
  const valid = isHex(value)
  const ratio = valid ? contrastRatio(value, against) : 0
  return (
    <Field label={label}>
      <div className="flex items-center gap-2">
        <input
          type="color"
          aria-label={`${label} picker`}
          value={valid ? value : '#000000'}
          onChange={(e) => onChange(e.target.value)}
          className="h-10 w-12 cursor-pointer rounded-control border border-field-line bg-field p-1"
        />
        <Input aria-label={`${label} hex`} value={value} onChange={(e) => onChange(e.target.value)} maxLength={7} className="w-28 font-mono" aria-invalid={!valid} />
        {valid && (
          <span className={`text-xs ${ratio >= AA ? 'text-win' : 'text-danger'}`}>
            {ratio.toFixed(1)}:1 {ratio >= AA ? 'passes AA' : 'too low for text'}
          </span>
        )}
      </div>
    </Field>
  )
}

export function Settings() {
  const admin = useIsAdmin()
  const client = useQueryClient()
  const { data: saved } = useAppearance()
  const [draft, setDraft] = useState<Appearance | null>(null)
  const current = draft ?? saved ?? DEFAULT_APPEARANCE

  useEffect(() => {
    applyAppearance(current)
  }, [current])
  useEffect(() => () => {
    const latest = client.getQueryData<Appearance>(appearanceKey)
    if (latest) applyAppearance(latest)
  }, [client])

  const store = (next: Appearance) => {
    client.setQueryData(appearanceKey, next)
    setDraft(null)
  }
  const save = useMutation({ mutationFn: () => api.saveAppearance(current), onSuccess: store })
  const reset = useMutation({ mutationFn: api.resetAppearance, onSuccess: store })

  if (!admin) return <Navigate to="/login" replace />

  const set = <K extends keyof Appearance>(key: K, value: Appearance[K]) => setDraft({ ...current, [key]: value })
  const valid = isHex(current.accent_light) && isHex(current.accent_dark) && current.site_name.trim().length > 0
  const dirty = draft !== null
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (valid) save.mutate()
  }

  return (
    <form onSubmit={submit} className="flex max-w-2xl flex-col gap-8">
      <div>
        <PageTitle>Appearance</PageTitle>
        <p className="mt-2 text-muted">Changes preview live. They apply to everyone once saved.</p>
      </div>
      <Field label="Site name">
        <Input value={current.site_name} onChange={(e) => set('site_name', e.target.value)} maxLength={40} className="max-w-sm" />
      </Field>
      <div className="flex flex-wrap gap-8">
        <ColorField label="Accent, light mode" value={current.accent_light} onChange={(v) => set('accent_light', v)} against={PAGE_BACKGROUND.light} />
        <ColorField label="Accent, dark mode" value={current.accent_dark} onChange={(v) => set('accent_dark', v)} against={PAGE_BACKGROUND.dark} />
      </div>
      <div className="flex flex-wrap items-end gap-8">
        <Field label={`Corner radius, ${current.radius}px`}>
          <input
            type="range"
            min={0}
            max={20}
            value={current.radius}
            onChange={(e) => set('radius', Number(e.target.value))}
            className="w-56 accent-accent"
          />
        </Field>
        <Field label="Typeface">
          <Select value={current.font} onChange={(e) => set('font', e.target.value as FontChoice)}>
            {(Object.keys(FONT_LABELS) as FontChoice[]).map((f) => (
              <option key={f} value={f}>
                {FONT_LABELS[f]}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <div className="flex flex-col gap-3 rounded-card border border-line bg-surface p-4">
        <p className="text-xs font-medium text-muted">Preview</p>
        <div className="flex flex-wrap items-center gap-3">
          <Button variant="primary" type="button">Primary action</Button>
          <Button type="button">Secondary</Button>
          <Input placeholder="Text field" className="w-48" />
          <span className="rounded-full border border-line px-2.5 py-0.5 text-xs font-medium text-accent">Live</span>
        </div>
      </div>
      <ErrorText error={save.error ?? reset.error} />
      <div className="flex gap-2">
        <Button type="submit" variant="primary" disabled={!valid || !dirty || save.isPending}>
          Save appearance
        </Button>
        <Button variant="ghost" onClick={() => setDraft(null)} disabled={!dirty}>
          Discard changes
        </Button>
        <Button variant="danger" onClick={() => reset.mutate()} disabled={reset.isPending}>
          Restore defaults
        </Button>
      </div>
    </form>
  )
}
