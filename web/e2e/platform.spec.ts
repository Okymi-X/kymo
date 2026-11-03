import { expect, test, type Page } from '@playwright/test'
import { PASSWORD } from './constants.js'

async function organizer(page: Page) {
  await page.goto('/login')
  await page.getByLabel('Password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Tournaments' })).toBeVisible()
}

test('a player registers, solves a challenge and reaches the scoreboard', async ({ page, browser }) => {
  // The organizer builds a solo Jeopardy event with one challenge and starts it.
  await organizer(page)
  await page.goto('/admin/events')
  await page.getByLabel('New event').fill('E2E Jeopardy')
  await page.getByLabel('Players').selectOption({ label: 'Solo' })
  await page.getByRole('button', { name: 'Create event' }).click()
  await expect(page.getByRole('heading', { name: 'E2E Jeopardy' })).toBeVisible()

  await page.getByRole('tab', { name: 'Challenges' }).click()
  await page.getByLabel('New category').fill('Web')
  await page.getByRole('button', { name: 'Add', exact: true }).click()
  await expect(page.getByText('Web', { exact: true }).first()).toBeVisible()
  await page.getByRole('button', { name: 'New challenge' }).click()
  const editor = page.getByRole('dialog')
  await editor.getByLabel('Title').fill('Warmup')
  await editor.getByLabel('Flag', { exact: true }).fill('KYMO{e2e}')
  await editor.getByRole('button', { name: 'Create challenge' }).click()
  await expect(editor).toBeHidden()

  await page.getByRole('button', { name: 'Warmup' }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Publish' }).click()
  await expect(page.getByRole('dialog').getByText('published', { exact: true })).toBeVisible()
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click()

  // Registration opens first: a live event only takes late registrations when it has a close time.
  await page.getByRole('tab', { name: 'Overview' }).click()
  await page.getByRole('button', { name: 'Move to registration open' }).click()
  await expect(page.getByText('Registration open', { exact: true }).first()).toBeVisible()

  // A new player, in a separate browser context, signs up and plays.
  const context = await browser.newContext()
  const player = await context.newPage()
  await player.goto('/signin?mode=register')
  await player.getByLabel('Username').fill('ada')
  await player.getByLabel(/^Password/).fill('a long player password')
  await player.getByRole('button', { name: 'Create account' }).click()
  await expect(player.getByRole('heading', { name: 'Events' })).toBeVisible()

  await player.getByRole('link', { name: /E2E Jeopardy/ }).click()
  await player.getByRole('button', { name: 'Register', exact: true }).click()
  await expect(player.getByText('You are registered')).toBeVisible()

  // The organizer starts the event; the player's page picks the state up.
  await page.getByRole('button', { name: 'Move to live' }).click()
  await expect(page.getByText('Live', { exact: true }).first()).toBeVisible()
  await player.reload()

  await player.getByRole('button', { name: /Warmup/ }).click()
  const dialog = player.getByRole('dialog')
  await dialog.getByLabel('Flag', { exact: true }).fill('KYMO{wrong}')
  await dialog.getByRole('button', { name: 'Submit' }).click()
  await expect(dialog.getByText(/Incorrect/)).toBeVisible()
  await dialog.getByLabel('Flag', { exact: true }).fill('KYMO{e2e}')
  await dialog.getByRole('button', { name: 'Submit' }).click()
  await expect(dialog.getByText('Correct, 100 points.')).toBeVisible()
  await dialog.getByRole('button', { name: 'Close' }).click()

  await player.getByRole('tab', { name: 'Scoreboard' }).click()
  const row = player.getByRole('row', { name: /ada/ })
  await expect(row).toBeVisible()
  await expect(row).toContainText('100')

  // The organizer sees the same activity in the monitor and audit trail.
  await page.goto('/monitor')
  await expect(page.getByRole('heading', { name: 'Monitor' })).toBeVisible()
  await expect(page.getByText('POST /api/v1/events/e2e-jeopardy/challenges/1/submit').first()).toBeVisible()
  await page.getByRole('tab', { name: 'Audit' }).click()
  await expect(page.getByRole('cell', { name: 'event_state' }).first()).toBeVisible()
  await context.close()
})

test('team members share a team and a visitor is asked to sign in', async ({ page, browser }) => {
  const context = await browser.newContext()
  const owner = await context.newPage()
  await owner.goto('/team')
  await expect(owner.getByText('You need to sign in')).toBeVisible()

  await owner.goto('/signin?mode=register&next=/team')
  await owner.getByLabel('Username').fill('grace')
  await owner.getByLabel(/^Password/).fill('a long player password')
  await owner.getByRole('button', { name: 'Create account' }).click()
  await expect(owner.getByRole('heading', { name: 'My team' })).toBeVisible()
  await owner.getByLabel('Team name').fill('Compilers')
  await owner.getByRole('button', { name: 'Create team' }).click()
  await expect(owner.getByRole('heading', { name: 'Compilers' })).toBeVisible()
  const code = (await owner.locator('code').first().textContent())?.trim() ?? ''
  expect(code).toMatch(/^[0-9a-f]{16}$/)

  const second = await browser.newContext()
  const member = await second.newPage()
  await member.goto('/signin?mode=register&next=/team')
  await member.getByLabel('Username').fill('linus')
  await member.getByLabel(/^Password/).fill('a long player password')
  await member.getByRole('button', { name: 'Create account' }).click()
  await member.getByLabel('Invite code').fill(code)
  await member.getByRole('button', { name: 'Join team' }).click()
  await expect(member.getByRole('heading', { name: 'Compilers' })).toBeVisible()
  await expect(member.getByText('@grace')).toBeVisible()
  // Only the owner sees the invite code.
  await expect(member.getByText('Invite code', { exact: true })).toHaveCount(0)
  await context.close()
  await second.close()
  void page
})

test('the VPN controls say plainly when the server has no VPN', async ({ page }) => {
  await organizer(page)
  await page.goto('/admin/events')
  await page.getByLabel('New event').fill('VPN Check')
  await page.getByLabel('Players').selectOption({ label: 'Solo' })
  await page.getByRole('button', { name: 'Create event' }).click()
  await expect(page.getByRole('heading', { name: 'VPN Check' })).toBeVisible()

  const network = page.getByLabel('Instance network')
  await expect(network).toHaveValue('published')
  await expect(page.getByRole('option', { name: /Private VPN/ })).toBeDisabled()
  await expect(page.getByRole('option', { name: /Private VPN/ })).toContainText('not enabled on this server')

  await page.goto('/admin/ops')
  await expect(page.getByRole('heading', { name: 'VPN gateway' })).toBeVisible()
  await expect(page.getByText(/Not enabled\. Set KYMO_VPN=1/)).toBeVisible()
})

test('closed sign-ups are explained up front and the server says so in plain words', async ({ page, browser }) => {
  await organizer(page)
  await page.goto('/admin/users')
  const toggle = page.getByLabel('Anyone can create a player account')
  await expect(toggle).toBeChecked()

  const context = await browser.newContext()
  const visitor = await context.newPage()
  try {
    await toggle.uncheck()
    await expect.poll(async () => (await (await page.request.get('/api/v1/registration')).json()).open).toBe(false)
    await visitor.goto('/signin?mode=register')
    // Told before typing anything, and the form falls back to signing in instead of a dead end.
    await expect(visitor.getByText(/Creating accounts is switched off on this server/)).toBeVisible()
    await expect(visitor.getByRole('heading', { name: 'Welcome back' })).toBeVisible()
    await expect(visitor.getByRole('button', { name: 'Sign in', exact: true })).toBeDisabled()

    // A direct attempt is refused with a sentence about sign-ups, not about tokens.
    const refused = await visitor.request.post('/api/v1/auth/register', { data: { username: 'latecomer', password: 'a long player password' } })
    expect(refused.status()).toBe(403)
    const message = ((await refused.json()) as { error: string }).error
    expect(message).toMatch(/sign-ups are closed/)
    expect(message).not.toMatch(/token/)
  } finally {
    await toggle.check()
    await expect.poll(async () => (await (await page.request.get('/api/v1/registration')).json()).open).toBe(true)
    await context.close()
  }
})

test('the sign-up form explains what is missing as you type', async ({ browser }) => {
  const context = await browser.newContext()
  const page = await context.newPage()
  await page.goto('/signin?mode=register')
  const create = page.getByRole('button', { name: 'Create account', exact: true })
  await expect(create).toBeDisabled()
  await expect(page.getByRole('alert')).toHaveCount(0) // an empty form does not open in red
  await page.getByLabel('Username').fill('ab')
  await expect(page.getByText('At least 3 characters.')).toBeVisible()
  await page.getByLabel('Username').fill('okay_name')
  await page.getByLabel(/^Password/).fill('short')
  await expect(page.getByText('5 more characters needed.')).toBeVisible()
  await expect(create).toBeDisabled()
  await page.getByLabel(/^Password/).fill('a long enough password')
  await expect(create).toBeEnabled()
  await page.getByLabel('Show the password').check()
  await expect(page.getByLabel(/^Password/)).toHaveAttribute('type', 'text')
  await context.close()
})

test('on a phone the main links live in a menu and the page does not scroll sideways', async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 } })
  const page = await context.newPage()
  await page.goto('/')
  await expect(page.getByRole('navigation', { name: 'Primary' })).toBeHidden()
  await page.getByRole('button', { name: 'Open menu' }).click()
  await expect(page.getByRole('navigation', { name: 'Menu' }).getByRole('link', { name: 'Tournaments' })).toBeVisible()
  await expect(page.getByRole('navigation', { name: 'Menu' }).getByRole('link', { name: 'Ladder' })).toBeVisible()
  await page.getByRole('navigation', { name: 'Menu' }).getByRole('link', { name: 'Events' }).click()
  await expect(page.getByRole('heading', { name: 'Events' })).toBeVisible()
  await expect(page.getByRole('navigation', { name: 'Menu' })).toBeHidden() // it closes after choosing
  expect(await page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')).toBe(true)
  await context.close()
})

test('an unknown address says so in general terms and offers a way on', async ({ page }) => {
  await page.goto('/this/does/not/exist')
  await expect(page.getByRole('heading', { name: 'Page not found' })).toBeVisible()
  await expect(page.getByText(/tournament may have been/)).toHaveCount(0)
  await expect(page.getByRole('link', { name: 'Go to the home page' })).toBeVisible()
  await page.goto('/e/no-such-event')
  await expect(page.getByRole('link', { name: 'Browse events' })).toBeVisible()
})

test('the search finds events as well as tournaments', async ({ page }) => {
  // Own event, so the test does not depend on another one having run.
  const login = await page.request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const token = ((await login.json()) as { token: string }).token
  const headers = { authorization: `Bearer ${token}` }
  const made = await page.request.post('/api/v1/admin/events', { headers, data: { name: 'Findable Spring CTF', kind: 'jeopardy', mode: 'solo' } })
  const event = (await made.json()) as { id: number }
  await page.request.post(`/api/v1/admin/events/${event.id}/state`, { headers, data: { state: 'registration_open' } })

  await page.goto('/')
  await expect(page.getByRole('button', { name: 'Open command palette' })).toContainText('Search Kymo')
  await page.keyboard.press('Control+k')
  const palette = page.getByRole('dialog', { name: 'Command palette' })
  await palette.getByRole('combobox').fill('findable')
  await expect(palette.getByRole('option', { name: /Findable Spring CTF/ })).toBeVisible()
  await page.keyboard.press('Enter')
  await expect(page.getByRole('heading', { name: 'Findable Spring CTF' })).toBeVisible()
})

test('the event page says what is actually true about signing in, creating an account and registering', async ({ page }) => {
  const login = await page.request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const create = async (name: string, states: string[]) => {
    const made = await page.request.post('/api/v1/admin/events', { headers, data: { name, kind: 'jeopardy', mode: 'solo' } })
    const event = (await made.json()) as { id: number; slug: string }
    for (const state of states) await page.request.post(`/api/v1/admin/events/${event.id}/state`, { headers, data: { state } })
    return event.slug
  }
  const open = await create('Wording Open', ['registration_open'])
  const started = await create('Wording Started', ['registration_open', 'live'])

  try {
    // Registration open and sign-ups open: sign in to register, or make an account first.
    await page.goto(`/e/${open}`)
    await expect(page.getByText('Sign in to register for this event.')).toBeVisible()
    await expect(page.getByRole('link', { name: 'Create account' })).toBeVisible()
    await expect(page.getByText('Sign in and register to see the challenges.')).toBeVisible()

    // Sign-ups closed: no Create account button, and it says who to ask.
    await page.request.put('/api/v1/admin/registration', { headers, data: { open: false } })
    await page.goto(`/e/${open}`)
    await expect(page.getByText('Sign in to register for this event.')).toBeVisible()
    await expect(page.getByRole('link', { name: 'Create account' })).toHaveCount(0)
    await expect(page.getByText(/ask an organizer/)).toBeVisible()

    // Registration itself closed (the event is live and has no late registration window).
    await page.goto(`/e/${started}`)
    await expect(page.getByText('Registration is not open for this event.')).toBeVisible()
    await expect(page.getByText(/event has already started/)).toBeVisible()
    await expect(page.getByRole('link', { name: 'Create account' })).toHaveCount(0)
  } finally {
    await page.request.put('/api/v1/admin/registration', { headers, data: { open: true } })
  }
})

test('a long event description says there is more', async ({ page }) => {
  const login = await page.request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const long = 'A long description that keeps going. '.repeat(8)
  const made = await page.request.post('/api/v1/admin/events', { headers, data: { name: 'Read More Cup', kind: 'jeopardy', mode: 'solo', description: long } })
  const event = (await made.json()) as { id: number }
  await page.request.post(`/api/v1/admin/events/${event.id}/state`, { headers, data: { state: 'registration_open' } })
  await page.goto('/events')
  const card = page.getByRole('link', { name: /Read More Cup/ })
  await expect(card.getByText('Read more', { exact: true })).toBeVisible()
  await card.click()
  await expect(page.getByText(long.trim())).toBeVisible()
})

test('administration pages fit a phone and keep their actions on screen', async ({ browser, request }) => {
  await request.post('/api/v1/auth/register', { data: { username: 'phoneuser', password: 'a long player password' } })
  const context = await browser.newContext({ viewport: { width: 390, height: 844 } })
  const page = await context.newPage()
  await organizer(page)
  const fits = async () => expect(await page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')).toBe(true)

  await page.goto('/admin/users')
  const ban = page.getByRole('button', { name: /^(Ban|Unban)$/ }).first()
  await expect(ban).toBeVisible()
  const box = await ban.boundingBox()
  expect(box).not.toBeNull()
  expect((box?.x ?? 999) + (box?.width ?? 999)).toBeLessThanOrEqual(390)
  await fits()

  await page.goto('/monitor')
  await expect(page.getByRole('heading', { name: 'Busiest endpoints' })).toBeVisible()
  await fits()
  for (const tab of ['Rate limits', 'Bans', 'Audit']) {
    await page.getByRole('tab', { name: tab }).click()
    await fits()
  }

  await page.goto('/admin/ops')
  await expect(page.getByRole('heading', { name: 'Background jobs' })).toBeVisible()
  await fits()
  await context.close()
})
