import { expect, test, type Page } from '@playwright/test'
import { PASSWORD } from './constants.js'

async function organizer(page: Page) {
  await page.goto('/login')
  await page.getByLabel('Password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Tournaments' })).toBeVisible()
}

test('a player registers, solves a challenge and reaches the scoreboard', async ({ page, browser }) => {
  test.setTimeout(120_000)
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

  // A refresh that fails while the dialog is open must not close it or lose what was typed.
  await dialog.getByLabel('Flag', { exact: true }).fill('KYMO{half typed')
  await player.route('**/api/v1/events/e2e-jeopardy/challenges', (route) => route.fulfill({ status: 429, contentType: 'application/json', body: '{"error":"too many requests"}' }))
  await player.waitForTimeout(21_000) // the board refreshes itself every 20 seconds, and this refresh now fails
  await expect(dialog).toBeVisible()
  await expect(dialog.getByLabel('Flag', { exact: true })).toHaveValue('KYMO{half typed')
  await player.unroute('**/api/v1/events/e2e-jeopardy/challenges')

  await dialog.getByLabel('Flag', { exact: true }).fill('KYMO{e2e}')
  await dialog.getByRole('button', { name: 'Submit' }).click()
  await expect(dialog.getByText('Correct, 100 points.')).toBeVisible()
  await dialog.getByRole('button', { name: 'Close' }).click()

  await player.getByRole('tab', { name: 'Scoreboard' }).click()
  const row = player.getByRole('row', { name: /ada/ })
  await expect(row).toBeVisible()
  await expect(row).toContainText('100')

  // The chart shows the same total, can hide a line, and reads out values on hover and by keyboard.
  await expect(player.getByRole('heading', { name: 'Score over time' })).toBeVisible()
  const legend = player.getByRole('list', { name: 'Lines' }).getByRole('button', { name: /ada/ })
  await expect(legend).toContainText('100')
  const chart = player.getByRole('img', { name: /Score over time for the top 1\. ada leads with 100 points/ })
  await expect(chart).toBeVisible()
  const box = await chart.boundingBox()
  await player.mouse.move((box?.x ?? 0) + (box?.width ?? 0) - 40, (box?.y ?? 0) + (box?.height ?? 0) / 2)
  await expect(player.getByText('ada').nth(0)).toBeVisible()
  await expect(player.locator('div.pointer-events-none', { hasText: '100' })).toBeVisible()
  await player.mouse.move(0, 0)
  await legend.click()
  await expect(legend).toHaveAttribute('aria-pressed', 'false')
  await legend.click()
  await expect(legend).toHaveAttribute('aria-pressed', 'true')
  await chart.focus()
  await player.keyboard.press('End')
  await expect(player.locator('div.pointer-events-none', { hasText: 'ada' })).toBeVisible()
  await player.keyboard.press('Escape')
  await expect(player.locator('div.pointer-events-none', { hasText: 'ada' })).toHaveCount(0)

  // The organizer sees the same activity in the monitor and audit trail.
  await page.goto('/monitor')
  await expect(page.getByRole('heading', { name: 'Monitor' })).toBeVisible()
  await expect(page.getByText('POST /api/v1/events/e2e-jeopardy/challenges/1/submit').first()).toBeVisible()
  await page.getByRole('tab', { name: 'Audit' }).click()
  await expect(page.getByRole('cell', { name: 'event_state' }).first()).toBeVisible()

  // The staff submission list hides the correct flag until it is revealed.
  await page.goto('/admin/events')
  await page.getByRole('link', { name: /E2E Jeopardy/ }).first().click()
  await page.getByRole('tab', { name: 'Submissions' }).click()
  await expect(page.getByText('KYMO{wrong}')).toBeVisible()
  await expect(page.getByText('KYMO{e2e}')).toHaveCount(0)
  await page.getByRole('button', { name: 'Reveal' }).first().click()
  await expect(page.getByText('KYMO{e2e}')).toBeVisible()
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
  await expect(page.getByRole('link', { name: 'Sign in' })).toHaveCount(1)
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

test('every event card looks the same and opens the full description', async ({ page }) => {
  const login = await page.request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const long = 'A long description that keeps going. '.repeat(8)
  const made = await page.request.post('/api/v1/admin/events', { headers, data: { name: 'Read More Cup', kind: 'jeopardy', mode: 'solo', description: long } })
  const event = (await made.json()) as { id: number }
  await page.request.post(`/api/v1/admin/events/${event.id}/state`, { headers, data: { state: 'registration_open' } })
  await page.goto('/events')
  const card = page.getByRole('link', { name: /Read More Cup/ })
  await expect(card.getByText('View event')).toBeVisible()
  expect(await page.getByRole('link', { name: /View event/ }).count()).toBe(await page.getByRole('heading', { level: 2 }).count())
  await card.click()
  await expect(page.getByText(long.trim())).toBeVisible()
})

test('closed sign-ups show the organizer contact on the event and sign in pages', async ({ page }) => {
  const login = await page.request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const made = await page.request.post('/api/v1/admin/events', { headers, data: { name: 'Landing Cup', kind: 'jeopardy', mode: 'solo' } })
  const event = (await made.json()) as { id: number; slug: string }
  await page.request.post(`/api/v1/admin/events/${event.id}/state`, { headers, data: { state: 'registration_open' } })
  try {
    await page.request.put('/api/v1/admin/contact', { headers, data: { contact: 'write to organizers@example.org' } })
    await page.request.put('/api/v1/admin/registration', { headers, data: { open: false } })
    await page.goto(`/e/${event.slug}`)
    await expect(page.getByText(/To get one: write to organizers@example.org/)).toBeVisible()
    await page.goto('/signin')
    await expect(page.getByText(/To get one: write to organizers@example.org/)).toBeVisible()
  } finally {
    await page.request.put('/api/v1/admin/registration', { headers, data: { open: true } })
    await page.request.put('/api/v1/admin/contact', { headers, data: { contact: '' } })
  }
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

test('an organizer creates an account without opening sign-ups, and correct flags stay hidden until revealed', async ({ page, request }) => {
  test.setTimeout(90_000)
  const login = await request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  await request.put('/api/v1/admin/registration', { headers, data: { open: false } })
  try {
    await organizer(page)
    await page.goto('/admin/users')
    // Sign-ups are closed, yet the organizer can make an account.
    await page.getByRole('button', { name: 'Create account' }).click()
    await page.getByLabel('Username').fill('walkin-player')
    await page.getByRole('button', { name: 'Create account' }).last().click()
    const temporary = (await page.getByTestId('temporary-password').textContent()) ?? ''
    expect(temporary).toHaveLength(16)
    await page.getByRole('button', { name: 'Done' }).click()
    await expect(page.getByText('@walkin-player').first()).toBeVisible()

    // That account can sign in with the temporary password; a wrong one says what it means.
    const visitor = await page.context().browser()!.newContext()
    const v = await visitor.newPage()
    await v.goto('/signin')
    await v.getByLabel('Username').fill('walkin-player')
    await v.getByLabel(/^Password/).fill('definitely the wrong one')
    await v.getByRole('button', { name: 'Sign in' }).last().click()
    await expect(v.getByText('Username or password is incorrect.')).toBeVisible()
    await v.getByLabel(/^Password/).fill(temporary)
    await v.getByRole('button', { name: 'Sign in' }).last().click()
    await expect(v.getByRole('link', { name: 'Events' }).first()).toBeVisible()
    await visitor.close()
  } finally {
    await request.put('/api/v1/admin/registration', { headers, data: { open: true } })
  }
})

test('the contact form appears exactly once on the Users page, before and after a sign-up change', async ({ page }) => {
  await organizer(page)
  await page.goto('/admin/users')
  const field = page.getByLabel('How people reach an organizer')
  await expect(field).toHaveCount(1)
  await page.getByLabel('Anyone can create a player account').click()
  await expect(field).toHaveCount(1)
  await page.getByLabel('Anyone can create a player account').click()
  await expect(field).toHaveCount(1)
  await expect(page.getByText(/Use Create account below/)).toHaveCount(0) // sign-ups are open again here
})

test('on the smallest phone nothing scrolls sideways and dialogs keep their title, actions and footer on screen', async ({ browser, request }) => {
  test.setTimeout(120_000)
  const login = await request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const made = await request.post('/api/v1/admin/events', { headers, data: { name: 'Tiny Phone Cup', kind: 'jeopardy', mode: 'solo' } })
  const event = (await made.json()) as { id: number }
  const category = (await (await request.post(`/api/v1/admin/events/${event.id}/categories`, { headers, data: { name: 'Web' } })).json()) as { id: number }
  const challenge = await request.post(`/api/v1/admin/events/${event.id}/challenges`, {
    headers,
    data: { category_id: category.id, title: 'Welcome', description: 'hi', scoring: { kind: 'static', points: 100 }, flag: { kind: 'static', value: 'KYMO{tiny}' }, flag_prefix: 'KYMO{', flag_suffix: '}' },
  })
  const challengeId = ((await challenge.json()) as { id: number }).id
  await request.post(`/api/v1/admin/challenges/${challengeId}/publish`, { headers })

  const context = await browser.newContext({ viewport: { width: 320, height: 568 } })
  const page = await context.newPage()
  const fits = async () => expect(await page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')).toBe(true)

  // The public pages, signed out.
  for (const path of ['/', '/events', '/ladder', '/signin']) {
    await page.goto(path)
    await expect(page.getByRole('main')).toBeVisible()
    await fits()
  }

  // Every part of a dialog stays inside it, and the title keeps a readable width.
  const geometry = async () =>
    (await page.evaluate(`(() => {
      const d = document.querySelector('dialog[open], [role="dialog"]')
      const box = d.getBoundingClientRect()
      const problems = []
      if (box.left < 0 || box.right > innerWidth + 1 || box.top < 0 || box.bottom > innerHeight + 1) problems.push('dialog outside the viewport')
      if (d.scrollWidth > d.clientWidth + 1) problems.push('dialog scrolls sideways')
      for (const part of d.querySelectorAll('header, footer, [data-modal-body], p.border-t')) {
        if (part.scrollWidth > part.clientWidth + 1) problems.push(part.tagName.toLowerCase() + ' scrolls sideways')
        for (const child of part.querySelectorAll('*')) {
          const r = child.getBoundingClientRect()
          if (r.width > 0 && (r.right > box.right + 1 || r.left < box.left - 1)) { problems.push('<' + child.tagName.toLowerCase() + '> crosses the dialog edge: ' + (child.textContent || '').slice(0, 20)); break }
        }
      }
      const title = d.querySelector('h2')
      return JSON.stringify({ problems, titleWidth: title ? Math.round(title.getBoundingClientRect().width) : null })
    })()`)) as string
  const check = async () => {
    const r = JSON.parse(await geometry()) as { problems: string[]; titleWidth: number | null }
    expect(r.problems).toEqual([])
    if (r.titleWidth !== null) expect(r.titleWidth).toBeGreaterThanOrEqual(110)
  }

  await organizer(page)
  await fits()
  // Creating an event: the host choice fits the screen.
  await page.goto('/admin/events')
  await expect(page.getByLabel('Instances run on')).toBeVisible()
  await fits()
  // The Lab hosts page: long names and addresses wrap instead of widening the page.
  const narrow = (await (await request.post('/api/v1/admin/labs', {
    headers,
    data: { name: 'Narrow lab host', api_url: 'https://lab-host-with-a-rather-long-name.example.org:7400', token: 'a-long-token-for-the-test-1234', public_host: 'lab-host-with-a-rather-long-name.example.org' },
  })).json()) as { lab: { id: number } }
  await page.goto('/admin/labs')
  await expect(page.getByRole('heading', { name: 'Narrow lab host' })).toBeVisible()
  await fits()
  await request.delete(`/api/v1/admin/labs/${narrow.lab.id}`, { headers })
  await page.goto(`/admin/events/${event.id}?view=challenges`)
  await page.getByRole('button', { name: /Welcome/ }).click()
  await expect(page.getByRole('dialog').getByRole('heading', { name: 'Edit challenge' })).toBeVisible()
  await check()
  await page.keyboard.press('Escape')

  await page.goto('/admin/users')
  await page.getByRole('button', { name: 'Create account' }).first().click()
  await expect(page.getByRole('dialog')).toBeVisible()
  await check()
  await page.keyboard.press('Escape')

  await page.goto('/')
  await page.keyboard.press('Control+k')
  const palette = page.getByRole('dialog', { name: 'Command palette' })
  await expect(palette).toBeVisible()
  await check()
  await context.close()
})

test('an organizer adds a lab host, sees it is unreachable, chooses it for an event and removes it safely', async ({ page, request }) => {
  test.setTimeout(120_000)
  const login = await request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const made = await request.post('/api/v1/admin/events', { headers, data: { name: 'Lab Choice Cup', kind: 'jeopardy', mode: 'solo' } })
  const event = (await made.json()) as { id: number }

  await organizer(page)
  await page.goto('/admin/labs')
  await expect(page.getByText('No lab host yet')).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Run challenges on a separate machine' })).toBeVisible()

  // Add one that nobody listens on: it is saved, and the answer says plainly that it did not answer.
  await page.getByRole('button', { name: 'Add a lab host' }).click()
  const dialog = page.getByRole('dialog')
  await dialog.getByLabel('Name').fill('Unplugged Lab')
  await dialog.getByLabel('API address').fill('http://127.0.0.1:1')
  await dialog.getByLabel('Token').fill('a-long-enough-lab-token-123')
  await expect(dialog.getByLabel('Public address')).toHaveValue('127.0.0.1') // suggested from the address
  await expect(dialog.getByText(/not encrypted/)).toBeVisible()
  await dialog.getByRole('button', { name: 'Add and test' }).click()
  await expect(dialog.getByRole('alert')).toContainText('did not answer')
  await dialog.getByRole('button', { name: 'Done' }).click()

  const card = page.getByRole('heading', { name: 'Unplugged Lab' }).locator('xpath=ancestor::section[1]')
  await expect(card.getByText('Unreachable')).toBeVisible()
  await expect(card.getByText('Not encrypted')).toBeVisible()
  await expect(page.getByText('a-long-enough-lab-token-123')).toHaveCount(0) // the token is never shown again
  await card.getByRole('button', { name: 'Test connection' }).click()
  await expect(card.getByRole('alert').first()).toBeVisible()

  // Editing does not ask for the token again.
  await card.getByRole('button', { name: 'Edit' }).click()
  await dialog.getByLabel('Name').fill('Unplugged Lab 2')
  await dialog.getByRole('button', { name: 'Save and test' }).click()
  await expect(dialog.getByText('Lab host updated')).toBeVisible()
  await dialog.getByRole('button', { name: 'Done' }).click()
  await expect(page.getByRole('heading', { name: 'Unplugged Lab 2' })).toBeVisible()

  // Operations lists it, as unreachable.
  await page.goto('/admin/ops')
  await expect(page.getByRole('region', { name: 'Lab hosts' }).getByText('Unplugged Lab 2')).toBeVisible()

  // The event can be moved to it, and the choice survives a reload.
  await page.goto(`/admin/events/${event.id}`)
  const where = page.getByLabel('Where instances run')
  // The choice says how busy the host is, so the admin can decide by what it has to spare.
  await where.selectOption({ label: 'Unplugged Lab 2 · 0 events, 0 running' })
  await expect(page.getByRole('combobox', { name: 'Where instances run' })).toHaveValue(/\d+/)
  await page.reload()
  await expect(page.getByLabel('Where instances run')).toHaveValue(/\d+/)

  // While an event uses it, it cannot be removed, and the reason is given.
  await page.goto('/admin/labs')
  const again = page.getByRole('heading', { name: 'Unplugged Lab 2' }).locator('xpath=ancestor::section[1]')
  await again.getByRole('button', { name: 'Remove' }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Remove' }).click()
  await expect(page.getByText(/still run their instances here/)).toBeVisible()
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click()

  await page.goto(`/admin/events/${event.id}`)
  await page.getByLabel('Where instances run').selectOption({ label: 'This server' })
  await expect(page.getByLabel('Where instances run')).toHaveValue('')
  await page.goto('/admin/labs')
  await page.getByRole('heading', { name: 'Unplugged Lab 2' }).locator('xpath=ancestor::section[1]').getByRole('button', { name: 'Remove' }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Remove' }).click()
  await expect(page.getByText('No lab host yet')).toBeVisible()
})

test('the audit log is searchable, filterable by what was recorded, and paged', async ({ page }) => {
  await organizer(page)
  await page.goto('/monitor')
  await page.getByRole('tab', { name: 'Audit' }).click()
  const search = page.getByRole('searchbox', { name: 'Search' })
  await expect(search).toBeVisible()
  // The action filter lists what has been recorded, with counts, not a fixed list.
  await expect(page.getByLabel('Action').locator('option').first()).toHaveText('All actions')
  await search.fill('no-such-address-anywhere')
  await expect(page.getByText('No entries match these filters.')).toBeVisible()
  await page.getByRole('button', { name: 'Clear filters' }).click()
  await expect(search).toHaveValue('')
  await expect(page.getByRole('table', { name: 'Audit log' })).toBeVisible()
  await expect(page.getByRole('columnheader', { name: 'From' })).toBeVisible()
  await expect(page.getByLabel('Per page')).toHaveValue('25')
})

test('the lab host is chosen when the event is created, with what each host is already doing', async ({ page, request }) => {
  const login = await request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const lab = (await (await request.post('/api/v1/admin/labs', {
    headers,
    data: { name: 'Pick me', api_url: 'http://127.0.0.1:9', token: 'a-long-token-for-the-test-1234', public_host: '127.0.0.1' },
  })).json()) as { lab: { id: number } }
  await organizer(page)
  await page.goto('/admin/events')
  await page.getByLabel('New event').fill('Placed at creation')
  await page.getByLabel('Players').selectOption({ label: 'Solo' })
  // The choice shows how busy each host is, so the admin can decide by what it has to spare.
  await expect(page.getByRole('option', { name: /Pick me · 0 events, 0 running/ })).toBeAttached()
  await page.getByLabel('Instances run on').selectOption({ label: 'Pick me · 0 events, 0 running' })
  await page.getByRole('button', { name: 'Create event' }).click()
  await expect(page.getByRole('heading', { name: 'Placed at creation' })).toBeVisible()
  await expect(page.getByLabel('Where instances run')).toHaveValue(String(lab.lab.id))
  // A tournament has no instances, so the choice is not offered for it.
  await page.goto('/admin/events')
  await page.getByLabel('Type').selectOption({ label: 'Tournament' })
  await expect(page.getByLabel('Instances run on')).toHaveCount(0)
  await request.delete(`/api/v1/admin/labs/${lab.lab.id}`, { headers }).catch(() => undefined)
})

test('a challenge can run on its own lab host while the event keeps its choice', async ({ page, request }) => {
  const login = await request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const lab = (await (await request.post('/api/v1/admin/labs', {
    headers,
    data: { name: 'Per challenge host', api_url: 'http://127.0.0.1:9', token: 'a-long-token-for-the-test-1234', public_host: '127.0.0.1' },
  })).json()) as { lab: { id: number } }
  const event = (await (await request.post('/api/v1/admin/events', { headers, data: { name: 'Mixed hosts', kind: 'jeopardy', mode: 'solo' } })).json()) as { id: number }
  const category = (await (await request.post(`/api/v1/admin/events/${event.id}/categories`, { headers, data: { name: 'Web' } })).json()) as { id: number }
  const made = await request.post(`/api/v1/admin/events/${event.id}/challenges`, {
    headers,
    data: {
      category_id: category.id,
      title: 'Own host',
      description: '',
      scoring: { kind: 'static', points: 100 },
      flag: { kind: 'static', value: 'KYMO{x}' },
      flag_prefix: '',
      flag_suffix: '',
      case_sensitive: true,
      max_attempts: null,
      release_at: null,
      retire_at: null,
      position: 0,
      requires_instance: true,
      instance_config: { image: 'registry.example.org/ctf/web:1.0', port: 8000, cpu_millis: 500, memory_mb: 256, pids: 128, ttl_secs: 1800 },
    },
  })
  const challenge = (await made.json()) as { id: number }
  await organizer(page)
  await page.goto(`/admin/events/${event.id}`)
  await page.getByRole('tab', { name: 'Challenges' }).click()
  await page.getByRole('button', { name: 'Own host' }).click()
  const runsOn = page.getByLabel('Runs on')
  await expect(runsOn).toHaveValue('')
  await runsOn.selectOption({ label: 'Per challenge host · 0 events, 0 running' })
  await page.getByRole('button', { name: 'Save changes' }).click()
  // The editor closes once both the challenge and its host are saved.
  await expect(runsOn).toHaveCount(0)
  const after = await request.get(`/api/v1/admin/challenges/${challenge.id}`, { headers })
  expect(((await after.json()) as { lab_host_id: number | null }).lab_host_id).toBe(lab.lab.id)
  await request.delete(`/api/v1/admin/challenges/${challenge.id}`, { headers }).catch(() => undefined)
  await request.delete(`/api/v1/admin/labs/${lab.lab.id}`, { headers }).catch(() => undefined)
})

test('an organizer on a public event page is told organizers cannot register, not asked to sign in', async ({ page, request }) => {
  const login = await request.post('/api/v1/auth/login', { data: { password: PASSWORD } })
  const headers = { authorization: `Bearer ${((await login.json()) as { token: string }).token}` }
  const made = (await (await request.post('/api/v1/admin/events', { headers, data: { name: 'Organizer view', kind: 'jeopardy', mode: 'solo' } })).json()) as { id: number; slug: string }
  await request.post(`/api/v1/admin/events/${made.id}/state`, { headers, data: { state: 'registration_open' } })
  await organizer(page)
  await page.goto(`/e/${made.slug}`)
  await expect(page.getByText('You are signed in as an organizer.')).toBeVisible()
  await expect(page.getByText('Sign in to register for this event.')).toHaveCount(0)
})

test('the header fits a phone that also shows a scrollbar, signed out and as an organizer', async ({ browser }) => {
  // 305 is 320 minus a classic scrollbar: the width many desktop browsers leave a narrow window.
  const context = await browser.newContext({ viewport: { width: 305, height: 700 } })
  const page = await context.newPage()
  for (const path of ['/', '/events', '/ladder', '/signin']) {
    await page.goto(path)
    await expect(page.getByRole('main')).toBeVisible()
    expect(await page.evaluate('document.documentElement.scrollWidth <= document.documentElement.clientWidth'), path).toBe(true)
  }
  await organizer(page)
  for (const path of ['/admin/events', '/admin/ops', '/settings', '/monitor']) {
    await page.goto(path)
    await expect(page.getByRole('main')).toBeVisible()
    expect(await page.evaluate('document.documentElement.scrollWidth <= document.documentElement.clientWidth'), path).toBe(true)
  }
  await context.close()
})
