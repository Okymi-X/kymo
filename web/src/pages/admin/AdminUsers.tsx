import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { Role } from '../../api/platformTypes'
import { DataTable, Empty, Gate, Loading, Panel, Pill, TD, Toggle } from '../../components/platform/bits'
import { Pager } from '../../components/Pager'
import { Button, ErrorText, PageTitle, SearchField, Select } from '../../components/ui'
import { timeAgo } from '../../lib/time'
import { usePageTitle } from '../../lib/usePageTitle'

const ROLES: Role[] = ['player', 'referee', 'monitor', 'admin']

function Registration() {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['registration-open'], queryFn: platform.registrationOpen })
  const set = useMutation({
    mutationFn: (open: boolean) => platform.setRegistrationOpen(open),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['registration-open'] }),
  })
  if (!q.data) return null
  return (
    <Panel className="flex flex-col gap-2">
      <Toggle label="Anyone can create a player account" checked={q.data.open} onChange={(v) => set.mutate(v)} disabled={set.isPending} />
      <p className="text-sm text-muted">Turn this off to close sign-ups, for example once the participant list is final.</p>
      <ErrorText error={set.error} />
    </Panel>
  )
}

function Body() {
  const [q, setQ] = useState('')
  const [page, setPage] = useState(1)
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
