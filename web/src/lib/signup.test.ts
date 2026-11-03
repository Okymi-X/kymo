import { describe, expect, it } from 'vitest'
import { canSignUp, validateSignUp } from './signup'

describe('validateSignUp', () => {
  it('says nothing about an empty form, so it does not open in red', () => {
    expect(validateSignUp('', '')).toEqual({})
  })
  it('explains a bad username', () => {
    expect(validateSignUp('ab', '').username).toBe('At least 3 characters.')
    expect(validateSignUp('a'.repeat(33), '').username).toBe('At most 32 characters.')
    expect(validateSignUp('bad name', '').username).toBe('Only letters, numbers, _ and -.')
    expect(validateSignUp('ok_name-1', '').username).toBeUndefined()
  })
  it('counts the characters a short password is missing', () => {
    expect(validateSignUp('abc', 'short').password).toBe('5 more characters needed.')
    expect(validateSignUp('abc', 'ninechars').password).toBe('1 more character needed.')
    expect(validateSignUp('abc', 'tencharsok').password).toBeUndefined()
  })
})

describe('canSignUp', () => {
  it('needs both fields and no problem', () => {
    expect(canSignUp('', '')).toBe(false)
    expect(canSignUp('abc', 'short')).toBe(false)
    expect(canSignUp('a b', 'a long enough password')).toBe(false)
    expect(canSignUp(' abc ', 'a long enough password')).toBe(true)
  })
})
