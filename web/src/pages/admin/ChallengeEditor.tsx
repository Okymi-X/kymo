import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Trash2 } from 'lucide-react'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { AdminChallenge, Category, FlagSpec, LabArtifact } from '../../api/platformTypes'
import { Modal, ModalBody, ModalCancel, ModalFooter, ModalHeader, modalForm } from '../../components/Modal'
import { Pill, Toggle } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, Select, Textarea } from '../../components/ui'
import { resizeRows } from '../../lib/flags'
import { artifactsFor, deliveryOf, deliveryProblems, findArtifact, isMachine, manifestDefaults, undeliveredFlags, type Delivery } from '../../lib/delivery'
import { flagHint } from '../../lib/platform'
import { DfirItems } from './DfirItems'
import { CodeSection } from './challenge/CodeSection'
import { Extras } from './challenge/Extras'
import { FlagRows } from './challenge/FlagRows'
import { DeliverySection } from './challenge/DeliverySection'
import { Num, Section, When } from './challenge/parts'
import { applyResponder, blank, dynamicSummary, flagPointsTotal, fromChallenge, int, isMulti, toBody, toCodeConfig, usesRows, validate, type Draft, type Problems } from './challenge/draft'

interface Props {
  eventId: number
  /** A tournament's challenges are worth fixed points and have no hints. */
  eventKind: 'jeopardy' | 'tournament'
  categories: Category[]
  /** The lab host the event runs on; null is this server's own runtime. */
  eventLabHostId: number | null
  challenge: AdminChallenge | null
  onClose: () => void
}

/** Fields that are only empty because nothing was typed yet: they complain after Save was tried, not before. */
const QUIET = ['title', 'category_id', 'image', 'flagValue', 'rows']

export function ChallengeEditor({ eventId, eventKind, categories, eventLabHostId, challenge, onClose }: Props) {
  const tournament = eventKind === 'tournament'
  const client = useQueryClient()
  const [start] = useState<Draft>(() => (challenge ? fromChallenge(challenge) : { ...blank(categories[0]?.id ?? 0), ...(tournament ? { points: '1' } : {}) }))
  const [d, setD] = useState<Draft>(start)
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setD((prev) => ({ ...prev, [key]: value }))
  const dirty = JSON.stringify(d) !== JSON.stringify(start)
  const dfir = d.kind === 'dfir'
  const coding = d.kind === 'code'
  // The flag, its delivery and the flag test belong to a plain flag challenge only.
  const plainFlag = d.kind === 'flag'
  const multi = isMulti(d)
  const rowsMode = usesRows(d)
  const fixedPoints = tournament || dfir || multi
  // The flags are fixed once a run or a solve exists (the server enforces it for runs too).
  const flagsLocked = challenge !== null && challenge.solves > 0
  const closing = d.suffix
  const setFlagMode = (mode: Draft['flagMode']) =>
    setD((prev) => ({
      ...prev,
      flagMode: mode,
      // A dynamic flag needs its own closing text: start from the accepted suffix.
      rows: mode === 'dynamic' ? prev.rows.map((r) => (r.closing === '' ? { ...r, closing } : r)) : prev.rows,
    }))
  const setFlagCount = (value: string) =>
    setD((prev) => ({ ...prev, flagCount: value, rows: resizeRows(prev.rows, Math.trunc(Number(value)) || 1, prev.suffix) }))
  const setResponder = (on: boolean) => setD((prev) => applyResponder(prev, on))
  const labs = useQuery({ queryKey: ['lab-choices'], queryFn: platform.labChoices })
  const labList = labs.data?.labs ?? []
  const eventLab = labList.find((l) => l.id === eventLabHostId) ?? null
  // Code runs on the event's lab host; an instance on its own choice, else the event's.
  const effectiveLabId = coding ? eventLabHostId : d.labHost === '' ? eventLabHostId : Number(d.labHost)
  const needsLab = d.requiresInstance || coding
  const catalogQuery = useQuery({
    queryKey: ['lab-catalog', effectiveLabId],
    queryFn: () => platform.labCatalog(effectiveLabId as number),
    enabled: needsLab && effectiveLabId !== null,
    staleTime: 30_000,
  })
  const catalog = effectiveLabId === null || !needsLab ? null : (catalogQuery.data ?? null)
  const artifact = catalog?.ok ? findArtifact(catalog.artifacts ?? [], d.image) : undefined
  // A saved challenge does not say whether it is a machine; the lab host's manifest for its image does.
  const delivery = deliveryOf(d.requiresInstance, artifact ? isMachine(artifact.runtime) : d.machine)
  const undelivered = d.requiresInstance && usesRows(d) ? undeliveredFlags(d.rows.slice(0, Math.max(1, int(d.flagCount))).map((r) => r.name), artifact) : []
  const place = deliveryProblems({ delivery, image: d.image, labId: effectiveLabId, catalog })
  const problems: Problems = { ...validate(d, tournament), ...place }
  const first = Object.values(problems)[0]
  // Show a field's problem only after it has been touched, so a new form does not open in red.
  const [touched, setTouched] = useState(false)
  // Empty required fields only complain once Save was tried, so picking a delivery does not open the form in red.
  const shown: Problems = touched ? problems : dirty ? Object.fromEntries(Object.entries(problems).filter(([key]) => !QUIET.includes(key))) : {}

  const setDelivery = (next: Delivery) =>
    setD((prev) => {
      if (next === 'none') return { ...prev, requiresInstance: false, machine: false, responder: false, image: '', labHost: '' }
      const machine = next === 'vm'
      // Switching between a container and a machine drops an image that belongs to the other kind.
      const keep = prev.requiresInstance && findArtifact(artifactsFor(next, catalog?.artifacts ?? []), prev.image) !== undefined
      return { ...prev, requiresInstance: true, machine, image: keep || prev.machine === machine ? prev.image : '' }
    })
  const pickArtifact = (a: LabArtifact) =>
    setD((prev) => {
      const next = { ...prev, image: a.artifact, machine: isMachine(a.runtime) }
      // A machine takes the manifest's numbers: the lab host runs the smaller of them and what is asked.
      if (isMachine(a.runtime)) {
        const m = manifestDefaults(a, { port: prev.port, cpu: prev.cpu, memory: prev.memory, pids: prev.pids })
        return { ...next, ...m, ttl: String(Math.min(Number(prev.ttl) || a.max_ttl_secs, a.max_ttl_secs)) }
      }
      return { ...next, port: a.port > 0 ? String(a.port) : prev.port }
    })
  const refresh = () => void client.invalidateQueries({ queryKey: ['admin-challenges', eventId] })
  const save = useMutation({
    mutationFn: async () => {
      const body = toBody(d, tournament)
      const saved = challenge ? await platform.updateChallenge(challenge.id, body) : await platform.createChallenge(eventId, body)
      if (coding) await platform.putCode(saved.id, toCodeConfig(d))
      // The host is its own call: it has its own checks (the lab host must work with the event's network mode).
      if (d.requiresInstance && d.labHost !== start.labHost) await platform.setChallengeLab(saved.id, d.labHost === '' ? null : Number(d.labHost))
      return saved
    },
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ['labs'] })
      refresh()
      onClose()
    },
  })
  const [confirmDelete, setConfirmDelete] = useState(false)
  const remove = useMutation({
    mutationFn: () => platform.deleteChallenge(challenge!.id),
    onSuccess: () => {
      refresh()
      onClose()
    },
    onSettled: () => setConfirmDelete(false),
  })
  // The server refuses a challenge with solves; the button says why before anyone tries.
  const whyNotDelete = !challenge
    ? null
    : challenge.state === 'published'
      ? 'Retire it first.'
      : !tournament && challenge.solves > 0
        ? `${challenge.solves} ${challenge.solves === 1 ? 'player has' : 'players have'} solved it. Keep it as retired instead.`
        : null
  const state = useMutation({
    mutationFn: (action: 'publish' | 'retire') => (action === 'publish' ? platform.publishChallenge(challenge!.id) : platform.retireChallenge(challenge!.id)),
    onSuccess: refresh,
  })

  return (
    <Modal label={challenge ? 'Edit challenge' : 'New challenge'} onClose={onClose} width="52rem" dirty={dirty} backdropCloses={false}>
      <form
        className={modalForm}
        onSubmit={(e) => {
          e.preventDefault()
          setTouched(true)
          if (!first) save.mutate()
        }}
      >
        <ModalHeader
          title={challenge ? 'Edit challenge' : 'New challenge'}
          subtitle={challenge && !tournament ? `${challenge.solves} ${challenge.solves === 1 ? 'solve' : 'solves'}` : 'Players see it once you publish it.'}
          actions={
            challenge && (
              <>
                <Pill tone={challenge.state === 'published' ? 'text-win' : 'text-muted'}>{challenge.state}</Pill>
                {challenge.state !== 'published' && (
                  <Button onClick={() => state.mutate('publish')} disabled={state.isPending || dirty} title={dirty ? 'Save your changes first' : undefined}>
                    Publish
                  </Button>
                )}
                {challenge.state === 'published' && (
                  <Button variant="danger" onClick={() => state.mutate('retire')} disabled={state.isPending}>
                    Retire
                  </Button>
                )}
                {confirmDelete ? (
                  <>
                    <Button variant="danger" onClick={() => remove.mutate()} disabled={remove.isPending}>
                      Delete for good
                    </Button>
                    <Button onClick={() => setConfirmDelete(false)} disabled={remove.isPending}>
                      Keep
                    </Button>
                  </>
                ) : (
                  <Button
                    aria-label="Delete challenge"
                    title={whyNotDelete ?? 'Delete this challenge. This cannot be undone.'}
                    onClick={() => setConfirmDelete(true)}
                    disabled={whyNotDelete !== null || state.isPending}
                  >
                    <Trash2 size={16} aria-hidden="true" />
                  </Button>
                )}
              </>
            )
          }
        />
        <ModalBody>
          <ErrorText error={state.error} />
          <ErrorText error={remove.error} />

          <Section title="Basics">
            <div className="grid gap-4 sm:grid-cols-[1fr_14rem_12rem]">
              <Field label="Title" error={shown.title}>
                <Input value={d.title} onChange={(e) => set('title', e.target.value)} maxLength={100} aria-invalid={shown.title ? true : undefined} />
              </Field>
              <Field label="Category" error={shown.category_id}>
                <Select value={d.category_id} onChange={(e) => set('category_id', int(e.target.value))}>
                  {categories.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Type" hint={challenge ? 'Fixed once created.' : dfir ? 'Several questions, each with its own answer and points.' : coding ? 'A program run against tests.' : undefined}>
                <Select value={d.kind} onChange={(e) => set('kind', e.target.value as Draft['kind'])} disabled={challenge !== null}>
                  <option value="flag">Single flag</option>
                  <option value="dfir">Multi-question</option>
                  {!tournament && <option value="code">Coding</option>}
                </Select>
              </Field>
            </div>
            <Field label="Description" hint="Markdown works: **bold**, *italic*, `code`, lists, > quotes, [links](https://…), and ``` code blocks. Say what to do, not how to solve it.">
              <Textarea value={d.description} onChange={(e) => set('description', e.target.value)} rows={4} />
            </Field>
            <Field label="Author (optional)" hint="Credited to players as “Created by …”. Leave empty to show nobody.">
              <Input value={d.author} onChange={(e) => set('author', e.target.value)} maxLength={80} />
            </Field>
          </Section>

          {plainFlag && (
            <DeliverySection
              d={d}
              set={set}
              delivery={delivery}
              onDelivery={setDelivery}
              onArtifact={pickArtifact}
              onResponder={setResponder}
              labs={labList}
              eventLab={eventLab}
              catalog={catalog}
              loading={catalogQuery.isFetching && catalog === null}
              shown={shown}
              problems={place}
              tournament={tournament}
              locked={flagsLocked}
            />
          )}

          {plainFlag && (
          <Section title={multi ? 'Flags' : 'Flag'} help="What a player must submit, and the shape that is accepted before it is even compared.">
            {(
              <>
                <div className="grid gap-4 sm:grid-cols-[14rem_10rem_1fr] sm:items-start">
                  <Field label="Flag mode" hint={flagsLocked || d.requiresInstance ? undefined : 'Dynamic: flag in template files.'}>
                    <Select value={d.flagMode} onChange={(e) => setFlagMode(e.target.value as Draft['flagMode'])} aria-label="Flag mode" disabled={flagsLocked}>
                      <option value="static">Static</option>
                      <option value="dynamic">Dynamic</option>
                    </Select>
                  </Field>
                  <Num label="Number of flags" value={d.flagCount} onChange={setFlagCount} min={1} error={shown.flagCount} />
                  <p className="text-xs text-muted sm:pt-6">
                    {d.flagMode === 'dynamic'
                      ? `Kymo generates a different flag for every instance run: your prefix, a random part, your closing text. A flag from another player never works. ${delivery === 'vm' ? 'It is written into the machine over SSH, where the manifest says.' : delivery === 'container' ? 'It is passed to the container as an environment variable, as the manifest says.' : 'It is put into the text files you attach as per-player templates.'}`
                      : multi
                        ? 'Players find each flag once. Points are split between the flags by weight.'
                        : 'One flag for everybody.'}
                  </p>
                </div>
                {flagsLocked && <p className="text-xs text-muted">Locked: it has runs or solves. Only labels can change.</p>}
              </>
            )}
            {rowsMode ? (
              <>
                <FlagRows d={d} set={set} shown={shown.rows} locked={flagsLocked} known={artifact?.flags ?? []} delivery={delivery} tournament={tournament} />
                {undelivered.length > 0 && (
                  <p className="text-xs text-danger">
                    The manifest for {artifact?.artifact} does not deliver {undelivered.join(', ')}: those flags would have nowhere to go. It delivers {artifact?.flags?.join(', ')}.
                  </p>
                )}
              </>
            ) : (
              <div className="grid gap-4 sm:grid-cols-[14rem_1fr]">
                <Field label="Kind">
                  <Select value={d.flagKind} onChange={(e) => set('flagKind', e.target.value as FlagSpec['kind'])}>
                    <option value="static">Exact text</option>
                    <option value="regex">Pattern (regex)</option>
                    <option value="per_team">Per team</option>
                  </Select>
                </Field>
                <Field label={d.flagKind === 'regex' ? 'Pattern' : 'Flag'} error={shown.flagValue} hint={d.flagKind === 'regex' ? 'Must match the whole submission.' : undefined}>
                  <Input value={d.flagValue} onChange={(e) => set('flagValue', e.target.value)} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={shown.flagValue ? true : undefined} />
                </Field>
              </div>
            )}
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="Accepted prefix">
                <Input value={d.prefix} onChange={(e) => set('prefix', e.target.value)} placeholder="KYMO{" className="font-mono" />
              </Field>
              <Field label="Accepted suffix">
                <Input value={d.suffix} onChange={(e) => set('suffix', e.target.value)} placeholder="}" className="font-mono" />
              </Field>
            </div>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <Toggle label="Case sensitive" checked={d.caseSensitive} onChange={(v) => set('caseSensitive', v)} />
              {flagHint(d.prefix, d.suffix) && <span className="text-xs text-muted">Players see the shape: <code className="font-mono text-ink">{flagHint(d.prefix, d.suffix)}</code></span>}
            </div>
          </Section>
          )}
          {coding && <CodeSection d={d} set={set} catalog={catalog} hasLab={eventLabHostId !== null} shown={shown} challenge={challenge} />}
          {dfir && (challenge ? <DfirItems challenge={challenge} onChanged={refresh} /> : <p className="rounded-card border border-dashed border-line p-4 text-sm text-muted">Create the challenge first, then add its questions here. Each question has its own answer, points and attempt limit.</p>)}

          <Section title="Points">
            <div className="grid gap-4 sm:grid-cols-[14rem_1fr] sm:items-start">
              <Field label="Scoring" hint={fixedPoints ? (dfir ? 'Split between the questions.' : multi ? (tournament ? 'The sum of the points of the flags.' : 'The total, split between the flags by weight.') : 'A match score is the points earned.') : undefined}>
                <Select value={fixedPoints ? 'static' : d.scoring} onChange={(e) => set('scoring', e.target.value as Draft['scoring'])} aria-label="Scoring type" disabled={fixedPoints}>
                  <option value="static">Fixed points</option>
                  <option value="dynamic">Dynamic points</option>
                </Select>
              </Field>
              {tournament && multi && !dfir ? (
                <p className="text-sm text-muted sm:pt-6">Worth {flagPointsTotal(d)} points in all: set the points of each flag above.</p>
              ) : fixedPoints || d.scoring === 'static' ? (
                <Num label="Points" value={d.points} onChange={(v) => set('points', v)} error={shown.points} />
              ) : (
                <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
                  <Num label="Start at" value={d.initial} onChange={(v) => set('initial', v)} error={shown.initial} />
                  <Num label="Floor" value={d.minimum} onChange={(v) => set('minimum', v)} error={shown.minimum} />
                  <Num label={d.curve === 'linear' ? 'Lost per solve' : 'Solves to floor'} value={d.decay} onChange={(v) => set('decay', v)} error={shown.decay} />
                  <Field label="Curve">
                    <Select value={d.curve} onChange={(e) => set('curve', e.target.value as Draft['curve'])}>
                      <option value="parabolic">Parabolic</option>
                      <option value="linear">Linear</option>
                      <option value="logarithmic">Logarithmic</option>
                    </Select>
                  </Field>
                </div>
              )}
            </div>
            {!fixedPoints && d.scoring === 'dynamic' && <p className="text-xs text-muted">{dynamicSummary(d)}</p>}
          </Section>

          <Section title="Availability" help="When players can see and solve it, and how many wrong tries they get. Times use your browser's time zone.">
            <div className="grid gap-4 sm:grid-cols-2">
              <When label="Release at" value={d.release} onChange={(v) => set('release', v)} hint="Empty: visible as soon as it is published." />
              <When label="Retire at" value={d.retire} onChange={(v) => set('retire', v)} error={shown.retire} hint="Empty: stays open until the event ends." />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              {!dfir && <Num label="Attempt limit" value={d.maxAttempts} onChange={(v) => set('maxAttempts', v)} min={1} error={shown.maxAttempts} hint="Empty means unlimited. Wrong-format answers never count." />}
              <Num label="Order" value={d.position} onChange={(v) => set('position', v)} min={-1000} hint="Lower numbers come first in the category." />
            </div>
          </Section>

          {challenge ? (
            <Extras challenge={challenge} tournament={tournament} onChanged={refresh} />
          ) : (
            <p className="rounded-card border border-dashed border-line p-4 text-sm text-muted">{tournament ? 'Files' : 'Hints, files'} {plainFlag ? 'and a flag test ' : ''}appear here once the challenge is created.</p>
          )}
          <ErrorText error={save.error} />
        </ModalBody>
        <ModalFooter note={first && (touched || dirty) ? <span className="text-danger">{first}</span> : dirty ? 'Unsaved changes' : null}>
          <ModalCancel />
          <Button type="submit" variant="primary" disabled={save.isPending || (challenge !== null && !dirty) || (dirty && first !== undefined)}>
            {challenge ? 'Save changes' : 'Create challenge'}
          </Button>
        </ModalFooter>
      </form>
    </Modal>
  )
}
