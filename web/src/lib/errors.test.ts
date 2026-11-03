import { describe, expect, it } from 'vitest'
import { friendlyMessage } from '../api/client'

describe('friendlyMessage', () => {
  it('words a failed sign in as incorrect credentials, not a missing session', () => {
    expect(friendlyMessage(401, 'authentication required', false, '/auth/player-login')).toBe('Username or password is incorrect.')
    expect(friendlyMessage(401, 'authentication required', true, '/auth/login')).toBe('That password is not correct.')
  })
  it('keeps a specific server message', () => {
    expect(friendlyMessage(403, 'sign-ups are closed on this server', false, '/auth/register')).toBe('sign-ups are closed on this server')
    expect(friendlyMessage(409, 'that username is taken', false, '/auth/register')).toBe('that username is taken')
  })
  it('tells a signed in person their session ended, but not on the sign in forms', () => {
    expect(friendlyMessage(401, 'authentication required', true, '/me')).toBe('Your session has ended. Sign in again.')
    expect(friendlyMessage(401, 'authentication required', true, '/me/anything')).toBe('Your session has ended. Sign in again.')
    expect(friendlyMessage(401, 'authentication required', true, '/auth/logout')).toBe('authentication required')
  })
  it('hides server internals', () => {
    expect(friendlyMessage(500, 'internal error', true, '/me')).toBe('Something went wrong on the server. Try again in a moment.')
    expect(friendlyMessage(502, 'bad gateway', false, '/vpn')).toBe('Something went wrong on the server. Try again in a moment.')
  })
  it('keeps the written-for-people message of a 503', () => {
    expect(friendlyMessage(503, 'the VPN gateway is not available, try again shortly', false, '/vpn/profile')).toBe('the VPN gateway is not available, try again shortly')
  })
  it('turns a bare not found into a sentence', () => {
    expect(friendlyMessage(404, 'not found', true, '/events/x')).toBe('That could not be found.')
  })
  it('has a fallback when the server said nothing', () => {
    expect(friendlyMessage(400, undefined, false, '/x')).toBe('The request failed. Try again.')
  })
})
