import { describe, expect, it } from 'vitest'
import type { Permission } from '../api/platformTypes'
import { changes, GROUPS, PERMISSION_INFO, sensitiveGrants, withPermission } from './permissions'

const ALL = Object.keys(PERMISSION_INFO) as Permission[]

describe('permissions', () => {
  it('describes every permission in a known group', () => {
    expect(ALL).toHaveLength(17)
    for (const p of ALL) {
      expect(PERMISSION_INFO[p].label.length).toBeGreaterThan(0)
      expect(PERMISSION_INFO[p].description.length).toBeGreaterThan(0)
      expect(GROUPS).toContain(PERMISSION_INFO[p].group)
    }
  })

  it('switches one permission and keeps the server order', () => {
    const next = withPermission(ALL, ['view_public', 'monitor'], 'play_event', true)
    expect(next).toEqual(['view_public', 'play_event', 'monitor'])
    expect(withPermission(ALL, next, 'monitor', false)).toEqual(['view_public', 'play_event'])
    // Switching on what is already on changes nothing.
    expect(withPermission(ALL, next, 'play_event', true)).toEqual(next)
  })

  it('reports what was granted and revoked', () => {
    expect(changes(['view_public', 'monitor'], ['view_public', 'manage_events'])).toEqual({
      granted: ['manage_events'],
      revoked: ['monitor'],
    })
    expect(changes(['view_public'], ['view_public'])).toEqual({ granted: [], revoked: [] })
  })

  it('flags the grants that need a second look', () => {
    expect(sensitiveGrants(['play_event', 'manage_users', 'manage_permissions'])).toEqual(['manage_users', 'manage_permissions'])
    expect(sensitiveGrants(['play_event', 'monitor'])).toEqual([])
  })
})
