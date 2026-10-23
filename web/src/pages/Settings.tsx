import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Check } from 'lucide-react'
import { Navigate } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import type { Appearance, Density, FontChoice, LineStyle, Match, Scheme, Slot, Tint } from '../api/types'
import { MatchCard } from '../components/MatchCard'
import { appearanceKey, useAppearance } from '../components/ThemeLoader'
import { BackLink } from '../components/BackLink'
import { Button, ErrorText, Field, Input, PageTitle, Segmented, StatusPill } from '../components/ui'
import { AA, DEFAULT_APPEARANCE, FONT_LABELS, LOGO_TYPES, PAGE_BACKGROUND, PRESETS, contrastRatio, isHex, matchingPreset, readLogo } from '../lib/theme'

const SAMPLE_NAMES = new Map([
  [1, 'Null Pointers'],
  [2, 'Stack Smashers'],
])

const SAMPLE_MATCH: Match = {
  id: 0,
  stage: 'winners',
  round: 1,
  position: 0,
  slots: [
    { kind: 'team', team: 1 },
    { kind: 'team', team: 2 },
  ],
  score: [3, 1],
  winner: 'A',
  forfeit: false,
  group: null,
  scheduled_at: null,
  venue: null,
  next_win: null,
}

const sampleName = (slot: Slot) => (slot.kind === 'team' ? (SAMPLE_NAMES.get(slot.team) ?? '') : 'TBD')

function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section aria-label={title} className="flex flex-col gap-4 border-t border-line pt-6 first:border-t-0 first:pt-0">
      <div>
        <h2 className="text-base font-semibold tracking-tight">{title}</h2>
        {hint && <p className="mt-0.5 text-sm text-muted">{hint}</p>}
      </div>
      {children}
    </section>
  )
}

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

function PresetPicker({ current, onPick }: { current: Appearance; onPick: (values: Appearance) => void }) {
  const active = matchingPreset(current)
  return (
    <div role="radiogroup" aria-label="Theme presets" className="grid grid-cols-2 gap-2 sm:grid-cols-4">
      {PRESETS.map((p) => (
        <button
          key={p.id}
          type="button"
          role="radio"
          aria-checked={active === p.id}
          onClick={() => onPick({ ...current, ...p.values })}
          className={`flex items-center gap-2.5 rounded-control border px-3 py-2.5 text-left text-sm font-medium transition-[border-color,box-shadow] duration-150 ${active === p.id ? 'border-accent shadow-[0_0_0_1px_var(--color-accent)]' : 'border-field-line hover:border-muted'}`}
        >
          <span className="flex shrink-0 -space-x-1.5" aria-hidden="true">
            <span className="size-5 rounded-full ring-2 ring-surface" style={{ background: p.values.accent_light }} />
            <span className="size-5 rounded-full ring-2 ring-surface" style={{ background: p.values.accent_dark }} />
          </span>
          <span className="flex-1 truncate">{p.name}</span>
          {active === p.id && <Check size={14} className="text-accent" aria-hidden="true" />}
        </button>
      ))}
    </div>
  )
}

function Preview({ look }: { look: Appearance }) {
  return (
    <div className="flex flex-col gap-4 rounded-card border border-line bg-surface p-5 shadow-[var(--shadow-card)]" aria-label="Live preview">
      <p className="text-xs font-medium text-muted">Live preview</p>
      <div className="flex items-center justify-between border-b border-line pb-3">
        <span className="font-semibold tracking-tight">{look.site_name || 'Site name'}</span>
        <span className="text-sm text-muted">Organizer sign in</span>
      </div>
      <div>
        <p className="text-2xl font-semibold leading-tight tracking-[-0.03em]">Tournaments</p>
        <p className="mt-1 text-sm text-muted">{look.tagline || 'Your tagline appears here.'}</p>
      </div>
      <div className="flex flex-col gap-3 rounded-card border border-line p-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="font-semibold tracking-tight">Spring Cup 2026</p>
            <p className="text-sm text-muted">Single elimination</p>
          </div>
          <StatusPill status="live" />
        </div>
        <div className="flex items-center gap-3">
          <div className="h-1 flex-1 overflow-hidden rounded-full bg-line">
            <div className="h-full w-2/3 rounded-full bg-accent" />
          </div>
          <span className="font-mono text-xs tabular-nums text-muted">4/6</span>
        </div>
      </div>
      <MatchCard match={SAMPLE_MATCH} teamName={sampleName} quiet />
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="primary" type="button">Start tournament</Button>
        <Button type="button">Export CSV</Button>
        <Input aria-label="Preview text field" placeholder="Team name" className="w-40" />
      </div>
    </div>
  )
}

const SCHEMES: { value: Scheme; label: string }[] = [
  { value: 'system', label: 'Match device' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
]

const TINTS: { value: Tint; label: string }[] = [
  { value: 'neutral', label: 'Neutral' },
  { value: 'warm', label: 'Warm' },
  { value: 'cool', label: 'Cool' },
]

const LINES: { value: LineStyle; label: string }[] = [
  { value: 'square', label: 'Square' },
  { value: 'curved', label: 'Curved' },
]

const AVATAR_OPTIONS: { value: 'on' | 'off'; label: string }[] = [
  { value: 'on', label: 'Show' },
  { value: 'off', label: 'Hide' },
]

const DENSITIES: { value: Density; label: string }[] = [
  { value: 'comfortable', label: 'Comfortable' },
  { value: 'compact', label: 'Compact' },
]

export function Settings() {
  const admin = useIsAdmin()
  const client = useQueryClient()
  const { data: saved } = useAppearance()
  const [draft, setDraft] = useState<Appearance | null>(null)
  const [logoError, setLogoError] = useState<string | null>(null)
  const current = draft ?? saved ?? DEFAULT_APPEARANCE

  useEffect(() => {
    if (draft) client.setQueryData(appearanceKey, draft)
  }, [draft, client])
  useEffect(
    () => () => {
      void client.invalidateQueries({ queryKey: appearanceKey })
    },
    [client],
  )

  const store = (next: Appearance) => {
    client.setQueryData(appearanceKey, next)
    setDraft(null)
  }
  const discard = () => {
    setDraft(null)
    void client.invalidateQueries({ queryKey: appearanceKey })
  }
  const save = useMutation({ mutationFn: () => api.saveAppearance(current), onSuccess: store })
  const reset = useMutation({ mutationFn: api.resetAppearance, onSuccess: store })
  const fonts = useMemo(() => (Object.keys(FONT_LABELS) as FontChoice[]).map((f) => ({ value: f, label: FONT_LABELS[f] })), [])

  if (!admin) return <Navigate to="/login" replace />

  const set = <K extends keyof Appearance>(key: K, value: Appearance[K]) => setDraft({ ...current, [key]: value })
  const valid = isHex(current.accent_light) && isHex(current.accent_dark) && current.site_name.trim().length > 0
  const dirty = draft !== null
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (valid) save.mutate()
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-10">
      <div className="flex flex-col gap-2">
        <BackLink>Tournaments</BackLink>
        <PageTitle>Appearance</PageTitle>
        <p className="max-w-xl text-lg text-muted">Pick a look for this instance. Changes preview on this page and apply to everyone once saved.</p>
      </div>

      <div className="grid gap-10 lg:grid-cols-[minmax(0,1fr)_24rem] lg:items-start">
        <div className="flex flex-col gap-8">
          <Section title="Theme" hint="Start from a preset, then adjust anything below.">
            <PresetPicker current={current} onPick={setDraft} />
          </Section>

          <Section title="Brand">
            <Field label="Site name">
              <Input value={current.site_name} onChange={(e) => set('site_name', e.target.value)} maxLength={40} className="max-w-sm" />
            </Field>
            <Field label="Logo">
              <div className="flex flex-wrap items-center gap-3">
                {current.logo && <img src={current.logo} alt="Current logo" className="h-9 w-auto max-w-48 rounded-control border border-line bg-surface p-1 object-contain" />}
                <input
                  type="file"
                  accept={LOGO_TYPES.join(',')}
                  aria-label="Upload logo"
                  className="text-sm file:mr-3 file:h-10 file:rounded-control file:border file:border-line file:bg-surface file:px-4 file:font-medium"
                  onChange={async (e) => {
                    const file = e.target.files?.[0]
                    e.target.value = ''
                    if (!file) return
                    const result = await readLogo(file)
                    if ('error' in result) setLogoError(result.error)
                    else {
                      setLogoError(null)
                      set('logo', result.uri)
                    }
                  }}
                />
                {current.logo && (
                  <Button type="button" variant="ghost" onClick={() => set('logo', null)}>
                    Remove logo
                  </Button>
                )}
              </div>
              <span className="text-xs text-muted">PNG, JPEG or WebP, up to 60 KB. Shown instead of the site name in the header.</span>
              {logoError && <span role="alert" className="text-sm text-danger">{logoError}</span>}
            </Field>
            <Field label="Tagline on the home page">
              <Input value={current.tagline} onChange={(e) => set('tagline', e.target.value)} maxLength={120} />
            </Field>
          </Section>

          <Section title="Mode" hint="Visitors follow their device unless you pick one for everyone.">
            <Segmented label="Color mode" value={current.scheme} options={SCHEMES} onChange={(v) => set('scheme', v)} />
          </Section>

          <Section title="Colors" hint="Buttons and highlights use the accent. Keep at least 4.5:1 so text stays readable.">
            <div className="flex flex-wrap gap-8">
              <ColorField label="Accent, light mode" value={current.accent_light} onChange={(v) => set('accent_light', v)} against={PAGE_BACKGROUND.light} />
              <ColorField label="Accent, dark mode" value={current.accent_dark} onChange={(v) => set('accent_dark', v)} against={PAGE_BACKGROUND.dark} />
            </div>
          </Section>

          <Section title="Shape and type">
            <div className="flex flex-wrap items-start gap-8">
              <Field label={`Corner radius, ${current.radius}px`}>
                <input type="range" min={0} max={20} value={current.radius} onChange={(e) => set('radius', Number(e.target.value))} className="h-10 w-56 accent-accent" />
              </Field>
              <Field label="Typeface">
                <Segmented label="Typeface" value={current.font} options={fonts} onChange={(v) => set('font', v)} />
              </Field>
              <Field label="Page tint">
                <Segmented label="Page tint" value={current.tint} options={TINTS} onChange={(v) => set('tint', v)} />
              </Field>
              <Field label="Bracket lines">
                <Segmented label="Bracket lines" value={current.lines} options={LINES} onChange={(v) => set('lines', v)} />
              </Field>
              <Field label="Team avatars">
                <Segmented label="Team avatars" value={current.avatars ? 'on' : 'off'} options={AVATAR_OPTIONS} onChange={(v) => set('avatars', v === 'on')} />
              </Field>
              <Field label="Density">
                <Segmented label="Density" value={current.density} options={DENSITIES} onChange={(v) => set('density', v)} />
              </Field>
            </div>
          </Section>
        </div>

        <div className="lg:sticky lg:top-20">
          <Preview look={current} />
        </div>
      </div>

      <div className="sticky bottom-4 z-10 flex flex-wrap items-center gap-2 rounded-card border border-line bg-surface/85 p-3 shadow-[var(--shadow-lift)] backdrop-blur-md">
        <p className="mr-auto px-2 text-sm text-muted" aria-live="polite">
          {dirty ? 'Unsaved changes' : 'All changes saved'}
        </p>
        <ErrorText error={save.error ?? reset.error} />
        <Button variant="danger" onClick={() => reset.mutate()} disabled={reset.isPending}>
          Restore defaults
        </Button>
        <Button variant="ghost" onClick={discard} disabled={!dirty}>
          Discard changes
        </Button>
        <Button type="submit" variant="primary" disabled={!valid || !dirty || save.isPending}>
          Save appearance
        </Button>
      </div>
    </form>
  )
}
