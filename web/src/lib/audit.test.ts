import { describe, expect, it } from 'vitest'
import { describeAction, describeTarget } from './audit'

describe('audit wording', () => {
  it('names the actions people take, with a kind', () => {
    expect(describeAction('participant_disqualify')).toEqual({ label: 'Disqualified from an event', kind: 'moderation' })
    expect(describeAction('lab_create').kind).toBe('config')
    expect(describeAction('organizer_login').kind).toBe('security')
  })
  it('makes something readable of an action it does not know', () => {
    expect(describeAction('rule_create')).toEqual({ label: 'Rule create', kind: 'other' })
    expect(describeAction('ip_ban_added').kind).toBe('moderation')
  })
  it('says what an entry was about', () => {
    expect(describeTarget('session', 'organizer password')).toBe('with the organizer password')
    expect(describeTarget('event', '13')).toBe('Event 13')
    expect(describeTarget('lab_host', '1')).toBe('Lab host 1')
  })
})
