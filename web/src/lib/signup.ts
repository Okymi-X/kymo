const USERNAME = /^[A-Za-z0-9_-]{3,32}$/
export const MIN_PASSWORD = 10

export interface SignUpProblems {
  username?: string
  password?: string
}

/** What the server would refuse, found while typing so the form can explain itself. */
export function validateSignUp(username: string, password: string): SignUpProblems {
  const problems: SignUpProblems = {}
  const name = username.trim()
  if (name !== '' && !USERNAME.test(name)) {
    problems.username = name.length < 3 ? 'At least 3 characters.' : name.length > 32 ? 'At most 32 characters.' : 'Only letters, numbers, _ and -.'
  }
  if (password !== '' && password.length < MIN_PASSWORD) {
    const missing = MIN_PASSWORD - password.length
    problems.password = `${missing} more ${missing === 1 ? 'character' : 'characters'} needed.`
  }
  return problems
}

/** Ready to send: both filled in and nothing to complain about. */
export function canSignUp(username: string, password: string): boolean {
  const problems = validateSignUp(username, password)
  return username.trim() !== '' && password !== '' && !problems.username && !problems.password
}
