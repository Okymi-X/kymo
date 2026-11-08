import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Lock } from 'lucide-react'
import { Fragment, useMemo, useState } from 'react'
import { platform } from '../../api/platform'
import type { Permission, PermissionMatrix, Role } from '../../api/platformTypes'
import { ConfirmDialog } from '../../components/ConfirmDialog'
import { DataTable, Gate, Loading, Panel, TD } from '../../components/platform/bits'
import { Button, ErrorText, PageTitle } from '../../components/ui'
import { changes, GROUPS, PERMISSION_INFO, ROLE_LABEL, sensitiveGrants, withPermission } from '../../lib/permissions'
import { usePageTitle } from '../../lib/usePageTitle'

type Draft = Partial<Record<Role, Permission[]>>

function Matrix({ matrix }: { matrix: PermissionMatrix }) {
  const client = useQueryClient()
  const [draft, setDraft] = useState<Draft>({})
  const [asking, setAsking] = useState<'save' | 'reset' | null>(null)

  const current = (role: Role): Permission[] => draft[role] ?? matrix.roles.find((r) => r.role === role)?.granted ?? []
  const edits = useMemo(
    () =>
      matrix.roles
        .filter((r) => draft[r.role] !== undefined)
        .map((r) => ({ role: r.role, ...changes(r.granted, draft[r.role] ?? r.granted) }))
        .filter((c) => c.granted.length > 0 || c.revoked.length > 0),
    [matrix, draft],
  )
  const count = edits.reduce((n, e) => n + e.granted.length + e.revoked.length, 0)
  const risky = edits.flatMap((e) => sensitiveGrants(e.granted).map((p) => `${PERMISSION_INFO[p].label} to ${ROLE_LABEL[e.role].toLowerCase()}s`))

  const done = async () => {
    await client.invalidateQueries({ queryKey: ['permissions'] })
    setDraft({})
    setAsking(null)
  }
  const save = useMutation({
    mutationFn: () => {
      const body: Draft = {}
      for (const e of edits) body[e.role] = current(e.role)
      return platform.savePermissions(body)
    },
    onSuccess: async (next) => {
      client.setQueryData(['permissions', 'matrix'], next)
      await done()
    },
  })
  const reset = useMutation({
    mutationFn: platform.resetPermissions,
    onSuccess: async (next) => {
      client.setQueryData(['permissions', 'matrix'], next)
      await done()
    },
  })

  const toggle = (role: Role, permission: Permission, on: boolean) => setDraft((d) => ({ ...d, [role]: withPermission(matrix.permissions, current(role), permission, on) }))
  const columns = matrix.roles

  return (
    <>
      <DataTable caption="Which role may do what" head={['Permission', ...columns.map((c) => ROLE_LABEL[c.role])]}>
        {GROUPS.map((group) => (
          <Fragment key={group}>
            <tr className="bg-line/30">
              <th scope="rowgroup" colSpan={columns.length + 1} className="px-4 py-2 text-left text-xs font-semibold uppercase tracking-wide text-muted">
                {group}
              </th>
            </tr>
            {matrix.permissions
              .filter((p) => PERMISSION_INFO[p].group === group)
              .map((p) => (
                <tr key={p}>
                  <td className={`${TD} min-w-56`}>
                    <span className="block font-medium">{PERMISSION_INFO[p].label}</span>
                    <span className="block text-xs text-muted">{PERMISSION_INFO[p].description}</span>
                  </td>
                  {columns.map((c) => {
                    const fixed = c.locked || c.forced.includes(p)
                    return (
                      <td key={c.role} className={`${TD} text-center`}>
                        <input
                          type="checkbox"
                          checked={current(c.role).includes(p)}
                          disabled={fixed}
                          onChange={(e) => toggle(c.role, p, e.target.checked)}
                          aria-label={`${ROLE_LABEL[c.role]}: ${PERMISSION_INFO[p].label}`}
                          title={fixed ? 'This cannot be changed' : undefined}
                          className="size-4 accent-[var(--color-accent)] disabled:opacity-50"
                        />
                      </td>
                    )
                  })}
                </tr>
              ))}
          </Fragment>
        ))}
      </DataTable>

      <div className="sticky bottom-3 z-10 flex flex-wrap items-center gap-3 rounded-card border border-line bg-surface p-3 shadow-[var(--shadow-lift)]">
        <p role="status" className="min-w-0 flex-1 text-sm text-muted">
          {count === 0 ? 'No unsaved changes.' : `${count} unsaved ${count === 1 ? 'change' : 'changes'}.`}
        </p>
        <Button variant="ghost" onClick={() => setAsking('reset')} disabled={reset.isPending}>
          Reset to defaults
        </Button>
        <Button variant="ghost" onClick={() => setDraft({})} disabled={count === 0}>
          Discard
        </Button>
        <Button variant="primary" onClick={() => (risky.length > 0 ? setAsking('save') : save.mutate())} disabled={count === 0 || save.isPending}>
          Save changes
        </Button>
      </div>
      <ErrorText error={save.error ?? reset.error} />

      {asking === 'save' && (
        <ConfirmDialog
          title="Hand out powerful permissions?"
          message={`You are giving ${risky.join(', ')}. People with these can change who has access, read or change flags, or reconfigure the site. Only continue if that is what you want.`}
          confirmLabel="Save changes"
          danger
          pending={save.isPending}
          onConfirm={() => save.mutate()}
          onClose={() => setAsking(null)}
        />
      )}
      {asking === 'reset' && (
        <ConfirmDialog
          title="Reset every role?"
          message="Player, referee and monitor go back to the permissions Kymo ships with. Unsaved changes are dropped."
          confirmLabel="Reset"
          danger
          pending={reset.isPending}
          onConfirm={() => reset.mutate()}
          onClose={() => setAsking(null)}
        />
      )}
    </>
  )
}

export function AdminPermissions() {
  usePageTitle('Permissions')
  const q = useQuery({ queryKey: ['permissions', 'matrix'], queryFn: platform.permissionMatrix })
  return (
    <Gate need="permission" permission="manage_permissions">
      <div className="flex flex-col gap-6">
        <PageTitle>Permissions</PageTitle>
        <Panel className="flex items-start gap-3 text-sm text-muted">
          <Lock size={18} aria-hidden="true" className="mt-0.5 shrink-0" />
          <p>
            Decide what each role may do. Changes apply at once, to everyone who already has that role, and are written to the audit log. Admin always keeps everything and public pages always stay public, so
            you cannot lock yourself out. A role that can manage permissions can also give itself more, so hand that one out with care.
          </p>
        </Panel>
        {q.isPending ? <Loading /> : q.data ? <Matrix matrix={q.data} /> : <ErrorText error={q.error} />}
      </div>
    </Gate>
  )
}
