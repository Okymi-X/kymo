import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { Role } from '../../api/platformTypes'
import { DataTable, Empty, Gate, Loading, Panel, Pill, TD, Toggle } from '../../components/platform/bits'
import { CreateUserDialog } from '../../components/platform/CreateUserDialog'
import { Pager } from '../../components/Pager'
import { Button, ErrorText, Field, Input, PageTitle, SearchField, Select } from '../../components/ui'
import { timeAgo } from '../../lib/time'
import { usePageTitle } from '../../lib/usePageTitle'

const ROLES: Role[] = ['player', 'referee', 'monitor', 'admin']

function Contact() {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['contact'], queryFn: platform.contact })
  const [text, setText] = useState<string | null>(null)
  const save = useMutation({
    mutationFn: (v: string) => platform.setContact(v),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['contact'] })
      void client.invalidateQueries({ queryKey: ['sign-ups-open'] })
      setText(null)
    },
  })
  if (!q.data) return null
  const value = text ?? q.data.contact
  return (
    <Panel className="flex flex-col gap-2">
      <Field label="How people reach an organizer" hint="Shown to visitors who cannot create an account, for example an email address or a chat handle. Leave empty to show nothing.">
        <div className="flex gap-2">
          <Input value={value} onChange={(e) => setText(e.target.value)} maxLength={200} placeholder="Ask on the event chat or write to organizers@example.org" className="min-w-0 flex-1" />
          <Button onClick={() => save.mutate(value)} disabled={text === null || save.isPending}>
            Save
          </Button>
        </div>
      </Field>
      <ErrorText error={save.error} />
    </Panel>
  )
}

function Registration() {
  const client = useQueryClient()
  const key = ['registration-open']
  const q = useQuery({ queryKey: key, queryFn: platform.registrationOpen })
  // The checkbox follows the click at once. It goes back to what the server says once the
  // request has finished, so a refusal puts it back where it was.
  const [local, setLocal] = useState<boolean | null>(null)
  const set = useMutation({
    mutationFn: (open: boolean) => platform.setRegistrationOpen(open),
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: key })
      void client.invalidateQueries({ queryKey: ['sign-ups-open'] })
      setLocal(null)
    },
  })
  if (!q.data) return null
  const open = local ?? q.data.open
  return (
    <Panel className="flex flex-col gap-2">
      <Toggle
        label="Anyone can create a player account"
        checked={open}
        onChange={(v) => {
          setLocal(v)
          set.mutate(v)
        }}
      />
      <p className="text-sm text-muted">
        {open
          ? 'Turn this off to close sign-ups, for example once the participant list is final. People who already have an account can still sign in.'
          : 'Sign-ups are closed. Visitors are told so on the sign-up page. Use Create account below to make accounts yourself; turn this on only if you want people to sign up on their own.'}
      </p>
      <ErrorText error={set.error} />
    </Panel>
  )
}

function Body() {
  const [q, setQ] = useState('')
  const [page, setPage] = useState(1)
  const [creating, setCreating] = useState(false)
  const client = useQueryClient()
  const users = useQuery({ queryKey: ['users', q, page], queryFn: () => platform.users({ q: q.trim() || undefined, page }) })
  const update = useMutation({
    mutationFn: (v: { id: number; role?: Role; banned?: boolean }) => platform.updateUser(v.id, { role: v.role, banned: v.banned }),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['users'] }),
  })
  const data = users.data
  const pages = data ? Math.max(1, Math.ceil(data.total / data.per_page)) : 1
  return (
    <div className="flex flex-col gap-6">
      <Registration />
      <Contact />
      <div className="flex justify-end">
        <Button variant="primary" onClick={() => setCreating(true)}>
          Create account
        </Button>
      </div>
      {creating && <CreateUserDialog onClose={() => setCreating(false)} />}
      <SearchField
        label="Search users"
        placeholder="Search by name"
        value={q}
        onChange={(e) => {
          setQ(e.target.value)
          setPage(1)
        }}
        className="max-w-sm"
      />
      {users.isPending ? (
        <Loading />
      ) : !data || data.items.length === 0 ? (
        <Empty>No users found.</Empty>
      ) : (
        <>
          {/* A wide table on larger screens; on a phone each account is a card, so role, status and
              the ban button are never pushed off the edge. */}
          <div className="hidden sm:block">
            <DataTable caption="Users" head={['User', 'Role', 'Joined', 'Status', '']}>
              {data.items.map((u) => (
                <tr key={u.id}>
                  <td className={TD}>
                    <span className="font-medium">{u.display_name || u.username}</span>
                    <span className="ml-2 text-xs text-muted">@{u.username}</span>
                  </td>
                  <td className={TD}>
                    <Select value={u.role} aria-label={`Role of ${u.username}`} onChange={(e) => update.mutate({ id: u.id, role: e.target.value as Role })} className="h-9 py-0">
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {r}
                        </option>
                      ))}
                    </Select>
                  </td>
                  <td className={`${TD} text-muted`}>{timeAgo(u.created_at)}</td>
                  <td className={TD}>{u.banned ? <Pill tone="text-danger">Banned</Pill> : <Pill>Active</Pill>}</td>
                  <td className={TD}>
                    <Button variant={u.banned ? 'secondary' : 'danger'} onClick={() => update.mutate({ id: u.id, banned: !u.banned })} disabled={update.isPending}>
                      {u.banned ? 'Unban' : 'Ban'}
                    </Button>
                  </td>
                </tr>
              ))}
            </DataTable>
          </div>
          <ul className="flex flex-col gap-3 sm:hidden" aria-label="Users">
            {data.items.map((u) => (
              <li key={u.id} className="flex flex-col gap-3 rounded-card border border-line bg-surface p-4">
                <div className="flex items-start justify-between gap-3">
                  <span className="min-w-0">
                    <span className="block truncate font-medium">{u.display_name || u.username}</span>
                    <span className="block truncate text-xs text-muted">
                      @{u.username} · joined {timeAgo(u.created_at)}
                    </span>
                  </span>
                  {u.banned ? <Pill tone="text-danger">Banned</Pill> : <Pill>Active</Pill>}
                </div>
                <div className="flex items-center gap-2">
                  <Select value={u.role} aria-label={`Role of ${u.username}`} onChange={(e) => update.mutate({ id: u.id, role: e.target.value as Role })} className="min-w-0 flex-1">
                    {ROLES.map((r) => (
                      <option key={r} value={r}>
                        {r}
                      </option>
                    ))}
                  </Select>
                  <Button variant={u.banned ? 'secondary' : 'danger'} onClick={() => update.mutate({ id: u.id, banned: !u.banned })} disabled={update.isPending}>
                    {u.banned ? 'Unban' : 'Ban'}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
          <Pager page={page} pages={pages} from={(page - 1) * data.per_page + 1} to={Math.min(data.total, page * data.per_page)} total={data.total} onChange={setPage} noun="users" />
        </>
      )}
      <ErrorText error={update.error} />
    </div>
  )
}

export function AdminUsers() {
  usePageTitle('Users')
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Users</PageTitle>
      <Gate need="admin">
        <Body />
      </Gate>
    </div>
  )
}
