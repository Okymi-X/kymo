import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { platform } from '../../api/platform'
import type { HomeContent } from '../../api/platformTypes'
import { DEFAULT_HOME } from '../../lib/home'
import { Toggle } from '../platform/bits'
import { Button, ErrorText, Field, Input, Segmented, Textarea } from '../ui'
import { CustomHome } from './CustomHome'
import { DefaultHome } from './DefaultHome'

const LINKS: [string, string][] = [
  ['/signin', 'Sign in'],
  ['/signin?mode=register', 'Create account'],
  ['/events', 'Events (needs sign in)'],
]

/** Where the organizer shapes the front page: the default page's texts, or their own HTML and CSS. */
export function HomeEditor() {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['home'], queryFn: platform.home })
  const [draft, setDraft] = useState<HomeContent>(DEFAULT_HOME)
  const [dirty, setDirty] = useState(false)
  useEffect(() => {
    if (q.data && !dirty) setDraft(q.data)
  }, [q.data, dirty])
  const set = <K extends keyof HomeContent>(key: K, value: HomeContent[K]) => {
    setDraft((d) => ({ ...d, [key]: value }))
    setDirty(true)
  }
  const stored = (home: HomeContent) => {
    client.setQueryData(['home'], home)
    setDraft(home)
    setDirty(false)
  }
  const save = useMutation({ mutationFn: () => platform.saveHome(draft), onSuccess: stored })
  const reset = useMutation({ mutationFn: platform.resetHome, onSuccess: stored })
  const custom = draft.mode === 'custom'
  return (
    <section aria-label="Home page" className="flex flex-col gap-5 border-t border-line pt-8">
      <div>
        <h2 className="text-xl font-semibold tracking-tight">Home page</h2>
        <p className="mt-0.5 max-w-2xl text-sm text-muted">
          The page visitors land on. Use the default page and change its texts, or write your own HTML and CSS. Players who are signed in see a greeting, their running instance and their events on top of it.
        </p>
      </div>
      <Segmented<HomeContent['mode']>
        label="Home page type"
        value={draft.mode}
        onChange={(m) => set('mode', m)}
        options={[
          { value: 'default', label: 'Default page' },
          { value: 'custom', label: 'My own HTML' },
        ]}
      />

      {!custom && (
        <div className="grid gap-4 lg:grid-cols-2">
          <Field label="Headline">
            <Input value={draft.headline} onChange={(e) => set('headline', e.target.value)} maxLength={200} />
          </Field>
          <Field label="Greeting for signed in players" hint="Their name follows it.">
            <Input value={draft.welcome} onChange={(e) => set('welcome', e.target.value)} maxLength={80} />
          </Field>
          <Field label="Text under the headline" hint="Leave empty to hide it.">
            <Textarea value={draft.subheadline} onChange={(e) => set('subheadline', e.target.value)} maxLength={2000} rows={3} />
          </Field>
          <Field label="Announcement banner" hint="Shown above everything. Leave empty for none.">
            <Textarea value={draft.banner} onChange={(e) => set('banner', e.target.value)} maxLength={2000} rows={3} />
          </Field>
          <Field label="Sign in button">
            <Input value={draft.signin_label} onChange={(e) => set('signin_label', e.target.value)} maxLength={40} />
          </Field>
          <Field label="Create account button">
            <Input value={draft.signup_label} onChange={(e) => set('signup_label', e.target.value)} maxLength={40} />
          </Field>
          <Field label="Events button (signed in players)">
            <Input value={draft.continue_label} onChange={(e) => set('continue_label', e.target.value)} maxLength={40} />
          </Field>
          <Field label="About title" hint="Leave the text empty to hide the section.">
            <Input value={draft.about_title} onChange={(e) => set('about_title', e.target.value)} maxLength={80} />
          </Field>
          <div className="lg:col-span-2">
            <Field label="About text">
              <Textarea value={draft.about_text} onChange={(e) => set('about_text', e.target.value)} maxLength={2000} rows={4} />
            </Field>
          </div>
          <Field label="Footer line">
            <Input value={draft.footer} onChange={(e) => set('footer', e.target.value)} maxLength={200} />
          </Field>
          <div className="flex flex-col justify-end gap-3">
            <Toggle label="Show the events of signed in players" checked={draft.show_events} onChange={(v) => set('show_events', v)} />
            <Toggle label="Show events with a public scoreboard" checked={draft.show_scoreboards} onChange={(v) => set('show_scoreboards', v)} />
          </div>
        </div>
      )}

      {custom && (
        <div className="flex flex-col gap-4">
          <p className="rounded-control border border-line bg-surface p-3 text-sm text-muted">
            Plain HTML and CSS. Scripts, forms, frames, inline <code className="font-mono">style</code> attributes and outside pictures or fonts are removed or blocked, and the CSS cannot use <code className="font-mono">@import</code>, <code className="font-mono">position: fixed</code> or backslash escapes. Style with classes in the CSS box. The CSS stays on this page only, and the theme colours are available, for example <code className="font-mono">var(--color-accent)</code>. Link to{' '}
            {LINKS.map(([href, label], i) => (
              <span key={href}>
                {i > 0 && ', '}
                <code className="font-mono">{href}</code> ({label})
              </span>
            ))}
            . The sign in and sign up forms themselves stay ours.
          </p>
          <Field label="HTML">
            <Textarea value={draft.custom_html} onChange={(e) => set('custom_html', e.target.value)} rows={14} maxLength={100000} spellCheck={false} className="font-mono text-sm" />
          </Field>
          <Field label="CSS">
            <Textarea value={draft.custom_css} onChange={(e) => set('custom_css', e.target.value)} rows={10} maxLength={50000} spellCheck={false} className="font-mono text-sm" />
          </Field>
        </div>
      )}

      <ErrorText error={save.error ?? reset.error} />
      <div className="flex flex-wrap items-center gap-3">
        <Button variant="primary" onClick={() => save.mutate()} disabled={save.isPending || !dirty}>
          Save home page
        </Button>
        <Button variant="danger" onClick={() => reset.mutate()} disabled={reset.isPending}>
          Restore the default page
        </Button>
        {dirty && <span className="text-sm text-muted">Unsaved changes. The preview below shows them.</span>}
        {!dirty && save.isSuccess && <span className="text-sm text-win">Saved</span>}
      </div>

      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-semibold">Preview</h3>
        <div className="rounded-card border border-line bg-bg p-4 sm:p-6">
          {custom && draft.custom_html.trim() !== '' ? <CustomHome html={draft.custom_html} css={draft.custom_css} /> : custom ? <p className="text-muted">Write some HTML to see it here.</p> : <DefaultHome home={draft} />}
        </div>
      </div>
    </section>
  )
}
