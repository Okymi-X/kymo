import { expect, test, type Page } from '@playwright/test'
import { PASSWORD } from './constants.js'

async function signIn(page: Page) {
  await page.goto('/login')
  await page.getByLabel('Password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Tournaments' })).toBeVisible()
}

/** The schedule shows one round at a time; this reveals every round when there are several. */
async function showAllRounds(page: Page) {
  const all = page.getByRole('button', { name: 'All rounds' })
  // A single round has no chips, so a short wait that may time out is the signal.
  if (await all.waitFor({ timeout: 1500 }).then(() => true, () => false)) await all.click()
}

async function record(page: Page, match: string, a: string, b: string) {
  await page.getByRole('button', { name: match }).click()
  const scores = page.getByRole('dialog').getByLabel('Score')
  await scores.nth(0).fill(a)
  await scores.nth(1).fill(b)
  await page.getByRole('button', { name: 'Save', exact: true }).click()
  await expect(page.getByRole('dialog')).toBeHidden()
}

test('organizer runs a four team knockout to a champion', async ({ page, browser }) => {
  await signIn(page)

  await page.getByLabel('New tournament').fill('E2E Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await expect(page.getByRole('heading', { name: 'E2E Cup' })).toBeVisible()

  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()

  await record(page, 'Edit result for Alpha versus Delta', '3', '1')
  await record(page, 'Edit result for Bravo versus Charlie', '0', '2')
  await record(page, 'Edit result for Alpha versus Charlie', '5', '4')
  await expect(page.getByText('Champion')).toBeVisible()
  await expect(page.getByText('Finished')).toBeVisible()

  const visitor = await browser.newPage()
  await visitor.goto('/t/e2e-cup')
  await expect(visitor.getByText('Champion')).toBeVisible()
  await expect(visitor.getByRole('button', { name: /Edit result/ })).toHaveCount(0)
  await visitor.close()
})

test('wrong password is rejected', async ({ page }) => {
  await page.goto('/login')
  await page.getByLabel('Password').fill('not the password')
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('alert')).toBeVisible()
})

test('command palette jumps to a tournament from the keyboard', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Palette Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await expect(page.getByRole('heading', { name: 'Palette Cup' })).toBeVisible()

  await page.goto('/')
  await page.keyboard.press('Control+k')
  await page.getByRole('dialog', { name: 'Command palette' }).getByRole('combobox').fill('palette')
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(/\/t\/palette-cup$/)
})

test('organizer awards a match by forfeit and can undo it', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Walkover Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()

  await page.getByRole('button', { name: 'Edit result for Alpha versus Delta' }).click()
  await page.getByRole('button', { name: 'Delta advances' }).click()
  await expect(page.getByRole('dialog')).toBeHidden()
  await expect(page.getByText('FF')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Edit result for Delta versus TBD' })).toHaveCount(0)

  await page.getByRole('button', { name: 'Edit result for Alpha versus Delta' }).click()
  await page.getByRole('button', { name: 'Clear result' }).click()
  await expect(page.getByText('FF')).toHaveCount(0)
})

test('third place match is played after the semifinals', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Bronze Cup')
  await page.getByLabel('Third place match').check()
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()

  await record(page, 'Edit result for Alpha versus Delta', '3', '1')
  await record(page, 'Edit result for Bravo versus Charlie', '0', '2')
  await expect(page.getByText('Third place', { exact: true })).toBeVisible()
  await record(page, 'Edit result for Delta versus Bravo', '1', '0')
  await record(page, 'Edit result for Alpha versus Charlie', '2', '1')
  await expect(page.getByText('Champion')).toBeVisible()
})

test('appearance changes are saved for everyone and can be restored', async ({ page, browser }) => {
  await signIn(page)
  await page.goto('/settings')
  await page.getByLabel('Site name').fill('HTB Club')
  await page.getByLabel('Accent, light mode hex').fill('#aa2200')
  await page.getByRole('button', { name: 'Save appearance' }).click()
  await expect(page.getByRole('link', { name: 'HTB Club home' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Start tournament' })).toHaveCSS('background-color', 'rgb(170, 34, 0)')

  const visitor = await browser.newPage()
  await visitor.goto('/')
  await expect(visitor.getByRole('link', { name: 'HTB Club home' })).toBeVisible()
  await expect(visitor).toHaveTitle('HTB Club')
  await visitor.close()

  await page.getByRole('button', { name: 'Restore defaults' }).click()
  await expect(page.getByRole('link', { name: 'Kymo home' })).toBeVisible()
})

test('standings, matches and team views show the same results', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Data Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha, 900\nBravo, 800\nCharlie, 700\nDelta, 600')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await record(page, 'Edit result for Alpha versus Delta', '3', '1')

  await page.getByRole('tab', { name: 'Standings' }).click()
  await expect(page).toHaveURL(/view=standings/)
  const alpha = page.getByRole('row', { name: /Alpha/ })
  await expect(alpha).toContainText('In')
  await expect(page.getByRole('row', { name: /Delta/ })).toContainText('Out')

  await page.getByRole('tab', { name: 'Matches' }).click()
  await expect(page.getByRole('heading', { name: /Up next/ })).toBeVisible()
  await expect(page.getByRole('heading', { name: /Played 1/ })).toBeVisible()

  await page.getByRole('tab', { name: 'Teams' }).click()
  await page.getByRole('button', { name: /Alpha/ }).click()
  const dialog = page.getByRole('dialog', { name: 'Alpha details' })
  await expect(dialog).toContainText('1-0')
  await expect(dialog).toContainText('Won 3:1')
  await dialog.getByRole('button', { name: 'Close' }).click()
  await expect(dialog).toBeHidden()

  await page.getByRole('tab', { name: 'Standings' }).click()
  const [download] = await Promise.all([page.waitForEvent('download'), page.getByRole('button', { name: 'Export CSV' }).click()])
  expect(download.suggestedFilename()).toBe('data-cup-standings.csv')
})

test('start errors stay visible and saving the roster keeps the editor state', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Bronze Guard')
  await page.getByLabel('Third place match').check()
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await expect(page.getByRole('button', { name: 'Start tournament' })).toBeDisabled()
  await expect(page.getByText('Needs at least 4 teams to start.')).toBeVisible()

  await page.getByLabel('Seeding on start').selectOption('random')
  await page.getByLabel(/Add teams/).fill('Charlie')
  await page.getByRole('button', { name: 'Save roster' }).click()
  await expect(page.getByLabel('Seeding on start')).toHaveValue('random')
  await expect(page.getByLabel(/Add teams/)).toHaveValue('Charlie')
})

test('site name previews in the header before saving', async ({ page }) => {
  await signIn(page)
  await page.goto('/settings')
  await page.getByLabel('Site name').fill('Preview Club')
  await expect(page.getByRole('link', { name: 'Preview Club home' })).toBeVisible()
  await page.getByRole('button', { name: 'Discard changes' }).click()
  await expect(page.getByRole('link', { name: 'Kymo home' })).toBeVisible()
})

test('sign out ends the session on the server', async ({ page, request }) => {
  await signIn(page)
  const token = await page.evaluate(() => sessionStorage.getItem('kymo.token'))
  const before = await request.get('/api/v1/admin/tokens', { headers: { authorization: `Bearer ${token}` } })
  expect(before.status()).toBe(200)
  await page.getByRole('button', { name: 'Organizer menu' }).click()
  await page.getByRole('menuitem', { name: 'Sign out' }).click()
  await expect(page.getByRole('link', { name: /Sign in|Organizer sign in/ }).first()).toBeVisible()
  const after = await request.get('/api/v1/admin/tokens', { headers: { authorization: `Bearer ${token}` } })
  expect(after.status()).toBe(401)
})

test('round robin plays with a draw and crowns the table leader', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('League Cup')
  await page.getByLabel('Format').selectOption('round_robin')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()

  await expect(page.getByRole('tab', { name: 'Schedule' })).toBeVisible()
  await page.getByRole('tab', { name: 'Bracket' }).click()
  await expect(page.getByRole('region', { name: 'Bracket map' }).getByRole('button', { name: /Edit result for/ })).toHaveCount(3)
  await page.getByRole('tab', { name: 'Schedule' }).click()
  await showAllRounds(page)

  const cards = page.getByRole('button', { name: /Edit result for/ })
  await expect(cards).toHaveCount(3)
  await cards.first().click()
  await page.getByRole('dialog').getByLabel('Score').nth(0).fill('1')
  await page.getByRole('dialog').getByLabel('Score').nth(1).fill('1')
  await page.getByRole('button', { name: 'Save', exact: true }).click()
  await expect(page.getByRole('dialog')).toBeHidden()

  for (let i = 0; i < 2; i++) {
    await page.getByRole('button', { name: /Edit result for/ }).nth(i + 1).click()
    await page.getByRole('dialog').getByLabel('Score').nth(0).fill('2')
    await page.getByRole('dialog').getByLabel('Score').nth(1).fill('0')
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('dialog')).toBeHidden()
  }
  await page.getByRole('tab', { name: 'Standings' }).click()
  await expect(page.getByRole('columnheader', { name: 'Pts' })).toBeVisible()
  await expect(page.getByText('Champion', { exact: true }).first()).toBeVisible()
  await expect(page.getByText('Finished')).toBeVisible()
})

test('groups then knockout plays the groups, fills the knockout and crowns a champion', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Worlds')
  await page.getByLabel('Format').selectOption('group_knockout')
  await page.getByLabel(/^Groups/).selectOption('2')
  await page.getByLabel(/^Advance per group/).selectOption('1')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('A1\nB1\nC1\nD1\nE1\nF1')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()

  await expect(page.getByRole('tab', { name: 'Groups' })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Group A' })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Group B' })).toBeVisible()

  await page.getByRole('tab', { name: 'Matches' }).click()
  for (let i = 0; i < 7; i++) {
    const next = page.getByRole('button', { name: /Group|Final/ }).first()
    await expect(next).toBeVisible()
    await next.click()
    await page.getByRole('dialog').getByLabel('Score').nth(0).fill('2')
    await page.getByRole('dialog').getByLabel('Score').nth(1).fill('1')
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('dialog')).toBeHidden()
  }
  await expect(page.getByText('Champion', { exact: true }).first()).toBeVisible()
  await expect(page.getByText('Finished')).toBeVisible()
  await page.getByRole('tab', { name: 'Knockout' }).click()
  await expect(page.getByText('Final', { exact: true }).first()).toBeVisible()
})

test('a saved look is in the very first paint, so a refresh never flips', async ({ page, browser }) => {
  await signIn(page)
  await page.goto('/settings')
  await page.getByRole('radio', { name: /Ember/ }).click()
  await page.getByRole('radio', { name: 'Dark' }).click()
  await page.getByLabel('Site name').fill('Night Club')
  await page.getByRole('button', { name: 'Save appearance' }).click()
  await expect(page.getByText('All changes saved')).toBeVisible()

  // A fresh visitor with JavaScript disabled still receives the saved look in the HTML itself.
  const context = await browser.newContext({ javaScriptEnabled: false })
  const response = await context.request.get('/')
  const html = await response.text()
  expect(html).toContain('data-scheme="dark"')
  expect(html).toContain('--color-accent:#c2410c')
  expect(html).toContain('<title>Night Club</title>')

  await context.close()

  // With JavaScript on, the saved look is already there while the settings request is still
  // in flight, so the page cannot flip when that request returns.
  const visitor = await browser.newPage()
  await visitor.route('**/api/v1/settings', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 3000))
    await route.continue()
  })
  await visitor.goto('/')
  await expect(visitor.getByRole('link', { name: 'Night Club home' })).toBeVisible({ timeout: 1500 })
  await expect(visitor.locator('html')).toHaveAttribute('data-scheme', 'dark')
  await visitor.close()

  await page.getByRole('button', { name: 'Restore defaults' }).click()
  await expect(page.getByRole('link', { name: 'Kymo home' })).toBeVisible()
})

test('organizer menu opens, is keyboard friendly and leads to the settings pages', async ({ page }) => {
  await signIn(page)
  const trigger = page.getByRole('button', { name: 'Organizer menu' })
  await trigger.click()
  await expect(page.getByRole('menuitem', { name: 'Appearance' })).toBeFocused()
  await page.keyboard.press('ArrowDown')
  await expect(page.getByRole('menuitem', { name: 'API tokens' })).toBeFocused()
  await page.keyboard.press('Escape')
  await expect(page.getByRole('menu')).toHaveCount(0)
  await expect(trigger).toBeFocused()

  await trigger.click()
  await page.getByRole('menuitem', { name: 'Appearance' }).click()
  await expect(page).toHaveURL(/\/settings$/)
  await expect(page.getByRole('navigation', { name: 'Primary' }).getByRole('link', { name: 'Tournaments' })).not.toHaveAttribute('aria-current', 'page')
  await page.goto('/')
  await expect(page.getByRole('navigation', { name: 'Primary' }).getByRole('link', { name: 'Tournaments' })).toHaveAttribute('aria-current', 'page')
})

test('the header shows the tournament once its title scrolls away', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Nav Cup')
  await page.getByLabel('Format').selectOption('double_elimination')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill(Array.from({ length: 16 }, (_, i) => `N${i + 1}`).join('\n'))
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await expect(page.getByRole('heading', { name: 'Nav Cup' })).toBeVisible()
  const header = page.getByRole('banner')
  await expect(header.getByText('Nav Cup')).toHaveCount(0)
  // The bracket is a bounded map now, so use the long list of matches to make the page scroll.
  await page.getByRole('tab', { name: 'Matches' }).click()
  await expect(page.getByRole('heading', { name: /Up next/ })).toBeVisible()
  await page.keyboard.press('End')
  await expect(header.getByText('Nav Cup')).toBeVisible()
  await page.keyboard.press('Home')
  await expect(header.getByText('Nav Cup')).toHaveCount(0)
})

test('activity log, following a team and the remembered choice', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Log Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await record(page, 'Edit result for Alpha versus Delta', '3', '1')
  await record(page, 'Edit result for Alpha versus Delta', '1', '3')

  await page.getByRole('tab', { name: 'Activity' }).click()
  const log = page.getByRole('list', { name: 'Activity' })
  await expect(log.getByText('Corrected:')).toBeVisible()
  await expect(log.getByText('Tournament started with 4 teams')).toBeVisible()
  await expect(log.getByRole('listitem')).toHaveCount(3)

  await page.getByRole('tab', { name: 'Teams' }).click()
  await page.getByRole('button', { name: /Delta/ }).click()
  await page.getByRole('button', { name: 'Follow this team' }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Close' }).click()
  const chip = page.getByRole('status').filter({ hasText: 'Following Delta' })
  await expect(chip).toBeVisible()
  await expect(chip).toContainText('Final')

  await page.reload()
  await expect(page.getByRole('status').filter({ hasText: 'Following Delta' })).toBeVisible()
  await page.getByRole('button', { name: 'Stop following Delta' }).click()
  await expect(page.getByRole('status').filter({ hasText: 'Following Delta' })).toHaveCount(0)
})

test('stage mode shows what is next, live results and the champion', async ({ page, browser }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Stage Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await record(page, 'Edit result for Alpha versus Delta', '3', '1')

  const screen = await browser.newPage({ viewport: { width: 1600, height: 900 } })
  await screen.goto('/t/stage-cup/stage')
  await expect(screen.getByRole('heading', { name: 'Stage Cup' })).toBeVisible()
  await expect(screen.getByRole('region', { name: 'Up next' })).toContainText('Bravo')
  await expect(screen.getByRole('region', { name: 'Latest' })).toContainText('beat')
  await expect(screen.getByRole('tab').first()).toHaveAttribute('aria-selected', 'true')
  await expect(screen.getByRole('list').filter({ hasText: 'Bravo' }).first()).toBeVisible()

  await screen.getByRole('button', { name: 'Pause rotation' }).click()
  await expect(screen.getByRole('button', { name: 'Resume rotation' })).toBeVisible()
  await screen.keyboard.press('ArrowRight')
  await expect(screen.getByRole('tab', { name: 'Bracket' })).toHaveAttribute('aria-selected', 'true')

  await record(page, 'Edit result for Bravo versus Charlie', '0', '2')
  await record(page, 'Edit result for Alpha versus Charlie', '5', '4')
  await expect(screen.getByText('Champion', { exact: true })).toBeVisible()
  await expect(screen.getByRole('region', { name: 'Up next' })).toHaveCount(0)
  await screen.close()
})

test('a match can be scheduled, shows on the lists and the follow chip, and is exported', async ({ page, request }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Plan Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()

  await page.getByRole('button', { name: 'Edit result for Alpha versus Delta' }).click()
  const soon = new Date(Date.now() + 30 * 60_000)
  const pad = (n: number) => String(n).padStart(2, '0')
  await page.getByLabel('Starts').fill(`${soon.getFullYear()}-${pad(soon.getMonth() + 1)}-${pad(soon.getDate())}T${pad(soon.getHours())}:${pad(soon.getMinutes())}`)
  await page.getByLabel('Place').fill('Room 7')
  await page.getByRole('button', { name: 'Save schedule' }).click()
  await expect(page.getByRole('dialog')).toBeHidden()

  await page.getByRole('tab', { name: 'Matches' }).click()
  await expect(page.getByRole('button', { name: /Alpha.*Delta/ })).toContainText('Room 7')
  await page.getByRole('tab', { name: 'Activity' }).click()
  await expect(page.getByRole('list', { name: 'Activity' })).toContainText('planned for')

  await page.getByRole('tab', { name: 'Teams' }).click()
  await page.getByRole('button', { name: /Alpha/ }).click()
  await page.getByRole('button', { name: 'Follow this team' }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Close' }).click()
  await expect(page.getByRole('status').filter({ hasText: 'Following Alpha' })).toContainText(/in (29|30) minutes/)

  const feed = await request.get('/api/v1/tournaments/plan-cup/calendar.ics')
  expect(await feed.text()).toContain('LOCATION:Room 7')
  await expect(page.getByRole('link', { name: 'Calendar' })).toBeVisible()
})

test('nav bar: live badge, search bar, light and dark switch that is remembered', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Live Badge Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await page.goto('/')

  const live = page.getByRole('link', { name: /tournaments? live now/ })
  await expect(live).toBeVisible()
  await live.click()
  await expect(page).toHaveURL(/status=live/)
  await expect(page.getByRole('button', { name: 'Live', exact: false }).first()).toHaveAttribute('aria-pressed', 'true')

  await expect(page.getByRole('button', { name: 'Open command palette' })).toContainText('Search tournaments')

  await page.getByRole('button', { name: 'Switch to dark mode' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-scheme', 'dark')
  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-scheme', 'dark')
  await page.getByRole('button', { name: 'Switch to light mode' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-scheme', 'light')
})

test('embed: dialog builds the snippet and the page works inside a real iframe', async ({ page, context }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Embed Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await record(page, 'Edit result for Alpha versus Delta', '3', '1')

  await page.getByRole('button', { name: 'Embed' }).click()
  const dialog = page.getByRole('dialog', { name: 'Embed this tournament' })
  await dialog.getByLabel('Theme').selectOption('dark')
  await dialog.getByLabel('Transparent background').check()
  const snippet = dialog.getByLabel('Code to paste')
  await expect(snippet).toHaveValue(/<iframe src="http:\/\/[^"]+\/embed\/embed-cup\?theme=dark&transparent=1"/)
  await dialog.getByRole('button', { name: 'Done' }).click()

  const origin = new URL(page.url()).origin
  // A host page on an http address (frame-ancestors * does not cover about:blank pages, and
  // Chrome blocks public looking hosts from framing localhost, so intercept a path on the same origin).
  await context.route(`${origin}/__host`, (route) =>
    route.fulfill({ contentType: 'text/html', body: `<h1>Host page</h1><iframe title="cup" src="${origin}/embed/embed-cup?theme=dark&transparent=1" width="900" height="500"></iframe>` }),
  )
  const host = await context.newPage()
  await host.goto(`${origin}/__host`)
  const frame = host.frameLocator('iframe[title="cup"]')
  await expect(frame.getByRole('heading', { name: 'Embed Cup' })).toBeVisible()
  await expect(frame.getByText('Null').or(frame.getByText('Alpha')).first()).toBeVisible()

  const bare = await context.newPage()
  await bare.goto('/embed/embed-cup?title=0&transparent=1')
  await expect(bare.getByRole('banner')).toHaveCount(0)
  await expect(bare.getByRole('heading', { name: 'Embed Cup' })).toHaveCount(0)
  await expect(bare.locator('body')).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)')
  await expect(bare.getByRole('link', { name: /Open on Kymo/ })).toBeVisible()
})

async function mapTransform(page: Page) {
  const style = (await page.locator('[data-map-canvas]').getAttribute('style')) ?? ''
  const m = /translate3d\((-?[\d.]+)px, (-?[\d.]+)px, 0px?\) scale\(([\d.]+)\)/.exec(style)
  if (!m) throw new Error(`no transform in ${style}`)
  return { x: Number(m[1]), y: Number(m[2]), s: Number(m[3]) }
}

test('the bracket is a map: drag, zoom at the cursor, keys, minimap, and no accidental clicks', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Map Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  const names = Array.from({ length: 16 }, (_, i) => `Team${String.fromCharCode(65 + i)}`).join('\n')
  await page.getByLabel(/Add teams/).fill(names)
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()

  const map = page.getByRole('region', { name: 'Bracket map' })
  await expect(map).toBeVisible()
  await expect(map.getByRole('button', { name: /Edit result for TeamA versus TeamP/ })).toBeVisible()
  const start = await mapTransform(page)
  const box = (await map.boundingBox())!
  // The whole map, controls and minimap included, fits under the page header without scrolling.
  expect(box.y + box.height).toBeLessThanOrEqual(page.viewportSize()!.height)

  // Dragging from a match card moves the map and does not open the result dialog.
  const card = map.getByRole('button', { name: /Edit result for TeamA versus TeamP/ })
  const cardBox = (await card.boundingBox())!
  await page.mouse.move(cardBox.x + 30, cardBox.y + 30)
  await page.mouse.down()
  await page.mouse.move(cardBox.x + 30 - 40, cardBox.y + 30 - 60, { steps: 6 })
  await page.mouse.up()
  await expect(page.getByRole('dialog')).toHaveCount(0)
  const dragged = await mapTransform(page)
  expect(dragged.y).not.toBe(start.y)

  // A plain click still opens the match.
  await map.getByRole('button', { name: 'Reset view' }).click()
  await expect.poll(async () => (await mapTransform(page)).y).toBe(start.y)
  await card.click()
  await expect(page.getByRole('dialog')).toBeVisible()
  await page.keyboard.press('Escape')

  // Ctrl and wheel zooms around the cursor.
  const before = await mapTransform(page)
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await page.keyboard.down('Control')
  await page.mouse.wheel(0, -240)
  await page.keyboard.up('Control')
  await expect.poll(async () => (await mapTransform(page)).s).toBeGreaterThan(before.s)

  // Keys pan and the reset button restores the first view.
  await map.focus()
  const keyed = await mapTransform(page)
  await page.keyboard.press('ArrowLeft')
  await expect.poll(async () => (await mapTransform(page)).x).not.toBe(keyed.x)
  await map.getByRole('button', { name: 'Reset view' }).click()
  await expect.poll(async () => (await mapTransform(page)).s).toBeCloseTo(start.s, 2)

  // Show everything zooms out until the whole bracket fits.
  await map.getByRole('button', { name: 'Show everything' }).click()
  await expect.poll(async () => (await mapTransform(page)).s).toBeLessThan(start.s)
})

test('an organizer can rename a tournament, and unknown pages show a 404', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Old Name')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByRole('button', { name: 'Rename' }).click()
  await page.getByLabel('Tournament name').fill('Fresh Name')
  await page.getByRole('button', { name: 'Save name' }).click()
  await expect(page.getByRole('heading', { name: 'Fresh Name' })).toBeVisible()

  await page.goto('/nowhere/at/all')
  await expect(page.getByRole('heading', { name: 'Page not found' })).toBeVisible()
  await page.keyboard.press('Shift+Slash')
  await expect(page.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeVisible()
})

test('a logo can be uploaded in appearance and shows in the header', async ({ page }) => {
  await signIn(page)
  await page.goto('/settings')
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==', 'base64')
  await page.getByLabel('Upload logo').setInputFiles({ name: 'logo.png', mimeType: 'image/png', buffer: png })
  await expect(page.getByAltText('Current logo')).toBeVisible()
  await page.getByRole('button', { name: 'Save appearance' }).click()
  await page.reload()
  await expect(page.getByRole('link', { name: 'Kymo home' }).locator('img')).toHaveAttribute('src', /^data:image\/png;base64,/)
  await page.getByRole('button', { name: 'Remove logo' }).click()
  await page.getByRole('button', { name: 'Save appearance' }).click()
  await expect(page.getByAltText('Current logo')).toHaveCount(0)
})

test('an organizer can decide a tie in a round robin table', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Tie League')
  await page.getByLabel('Format').selectOption('round_robin')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await page.getByRole('tab', { name: 'Schedule' }).click()
  await showAllRounds(page)
  for (let i = 0; i < 3; i++) {
    await page.getByRole('button', { name: /Edit result for/ }).nth(i).click()
    await page.getByRole('dialog').getByLabel('Score').nth(0).fill('1')
    await page.getByRole('dialog').getByLabel('Score').nth(1).fill('1')
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('dialog')).toBeHidden()
  }
  await page.getByRole('tab', { name: 'Standings' }).click()
  await page.getByRole('button', { name: 'Decide ties' }).click()
  await page.getByRole('button', { name: 'Move Charlie up' }).click()
  await page.getByRole('button', { name: 'Move Charlie up' }).click()
  await page.getByRole('button', { name: 'Save order' }).click()
  await expect(page.getByRole('table').getByRole('row').nth(1)).toContainText('Charlie')
})

test('a Swiss tournament plans the next round once a round is played', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Swiss Cup')
  await page.getByLabel('Format').selectOption('swiss')
  await page.getByLabel('Rounds').selectOption('2')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await page.getByRole('tab', { name: 'Schedule' }).click()
  await showAllRounds(page)
  const edits = page.getByRole('button', { name: /Edit result for/ })
  await expect(edits).toHaveCount(2)
  for (let i = 0; i < 2; i++) {
    await edits.nth(i).click()
    await page.getByRole('dialog').getByLabel('Score').nth(0).fill('2')
    await page.getByRole('dialog').getByLabel('Score').nth(1).fill('1')
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('dialog')).toBeHidden()
  }
  await showAllRounds(page)
  await expect(edits).toHaveCount(4)
  await expect(page.getByText('Round 2').first()).toBeVisible()
})

test('a tournament can get its own color and drop it again', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Colored Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByRole('button', { name: 'Color', exact: true }).click()
  await page.getByLabel('Hex color').fill('#aa2200')
  await page.getByRole('button', { name: 'Save color' }).click()
  await expect(page.locator('[data-tournament-accent]').first()).toHaveCSS('--tournament-accent', '#aa2200')
  await page.reload()
  await expect(page.locator('[data-tournament-accent]').first()).toBeVisible()
  await page.getByRole('button', { name: 'Color', exact: true }).click()
  await page.getByRole('button', { name: 'Use site color' }).click()
  await expect(page.locator('[data-tournament-accent]')).toHaveCount(0)
})

test('a tournament can be duplicated and a setup file imported', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Original Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Save roster' }).click()
  await expect(page.getByRole('button', { name: 'Save roster' })).toBeEnabled()
  await page.getByRole('button', { name: 'Duplicate' }).click()
  await expect(page.getByRole('heading', { name: 'Original Cup copy' })).toBeVisible()
  await expect(page.getByText(/4 teams/)).toBeVisible()

  await page.goto('/')
  const setup = JSON.stringify({ name: 'From File', format: 'swiss', teams: [{ name: 'A' }, { name: 'B' }, { name: 'C' }, { name: 'D' }] })
  await page.getByLabel('Import a tournament setup file').setInputFiles({ name: 'setup.json', mimeType: 'application/json', buffer: Buffer.from(setup) })
  await expect(page.getByRole('heading', { name: 'From File' })).toBeVisible()
})

test('the newest result can be undone from the activity log', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Undo Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await record(page, 'Edit result for Alpha versus Delta', '3', '1')
  await page.getByRole('tab', { name: 'Activity' }).click()
  await page.getByRole('button', { name: 'Undo', exact: true }).click()
  await page.getByRole('button', { name: 'Undo result' }).click()
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toHaveCount(0)
  await page.getByRole('tab', { name: 'Bracket' }).click()
  await expect(page.getByRole('button', { name: 'Edit result for Alpha versus Delta' })).toBeVisible()
})

test('the tab title follows the page and a skip link reaches the content', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Title Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await expect(page).toHaveTitle(/^Title Cup · /)
  await page.goto('/settings')
  await expect(page).toHaveTitle(/^Appearance · /)
  await page.keyboard.press('Tab')
  await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused()
})

test('a webhook can be added, tested and removed without showing its secret path', async ({ page }) => {
  await signIn(page)
  await page.goto('/tokens')
  await page.getByLabel('Label', { exact: true }).last().fill('Event chat')
  await page.getByLabel('Webhook URL').fill('http://127.0.0.1:9/secret-token-path')
  await page.getByRole('button', { name: 'Add webhook' }).click()
  const hooks = page.getByRole('region', { name: 'Webhooks' })
  await expect(hooks.getByText('Event chat')).toBeVisible()
  await expect(hooks.getByText('127.0.0.1:9')).toBeVisible()
  await expect(hooks).not.toContainText('secret-token-path')
  await hooks.getByRole('button', { name: 'Send test' }).click()
  await expect(hooks.getByRole('status')).toContainText(/could not be reached|answered/i)
  await hooks.getByRole('button', { name: 'Remove' }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Remove' }).click()
  await expect(hooks.getByText('No webhooks yet.')).toBeVisible()
})

test('a referee token signs in, records results and sees no organizer tools', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Referee Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await expect(page.getByRole('tab', { name: 'Standings' })).toBeVisible()

  await page.goto('/tokens')
  await page.getByLabel('Label', { exact: true }).first().fill('Table referee')
  await page.getByLabel('Access').selectOption('referee')
  await page.getByLabel('Referee Cup').check()
  await page.getByRole('button', { name: 'Create token' }).click()
  const secret = (await page.getByRole('status').locator('code').innerText()).trim()
  await expect(page.getByText(/Referee, 1 tournament/)).toBeVisible()

  await page.getByRole('button', { name: 'Organizer menu' }).click()
  await page.getByRole('menuitem', { name: 'Sign out' }).click()
  await page.goto('/login')
  await page.getByText('I am a referee with a token').click()
  await page.getByLabel('Referee token').fill(secret)
  await page.getByRole('button', { name: 'Sign in as referee' }).click()
  await expect(page.getByRole('button', { name: 'Referee, sign out' })).toBeVisible()

  await page.goto('/t/referee-cup')
  await expect(page.getByRole('button', { name: 'Rename' })).toHaveCount(0)
  await expect(page.getByRole('button', { name: 'Delete' })).toHaveCount(0)
  await record(page, 'Edit result for Alpha versus Delta', '3', '1')
  await expect(page.getByRole('button', { name: 'Edit result for Alpha versus Delta' })).toContainText('3')
})

test('a best of 3 series is entered game by game and decides the match', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Series Cup')
  await page.locator('select').filter({ hasText: 'Best of 3' }).selectOption('3')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await page.getByRole('button', { name: 'Edit result for Alpha versus Delta' }).click()
  const dialog = page.getByRole('dialog')
  await dialog.getByLabel('Game 1 score for Alpha').fill('10')
  await dialog.getByLabel('Game 1 score for Delta').fill('4')
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeDisabled()
  await dialog.getByRole('button', { name: 'Add game' }).click()
  await dialog.getByLabel('Game 2 score for Alpha').fill('3')
  await dialog.getByLabel('Game 2 score for Delta').fill('9')
  await dialog.getByRole('button', { name: 'Add game' }).click()
  await dialog.getByLabel('Game 3 score for Alpha').fill('12')
  await dialog.getByLabel('Game 3 score for Delta').fill('11')
  await expect(dialog.getByRole('button', { name: 'Add game' })).toBeDisabled()
  await dialog.getByRole('button', { name: 'Save', exact: true }).click()
  await expect(page.getByRole('dialog')).toBeHidden()
  await expect(page.getByRole('button', { name: 'Edit result for Alpha versus Delta' })).toContainText('2')
})

test('a match that is overdue shows in Late matches and can be awarded to the team that came', async ({ page }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Late Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await page.getByRole('button', { name: 'Edit result for Alpha versus Delta' }).click()
  const past = new Date(Date.now() - 25 * 60_000)
  const pad = (n: number) => String(n).padStart(2, '0')
  await page.getByLabel('Starts').fill(`${past.getFullYear()}-${pad(past.getMonth() + 1)}-${pad(past.getDate())}T${pad(past.getHours())}:${pad(past.getMinutes())}`)
  await page.getByRole('button', { name: 'Save schedule' }).click()
  await page.getByRole('tab', { name: 'Matches' }).click()
  const late = page.getByRole('region', { name: 'Late matches' })
  await expect(late).toContainText('Alpha versus Delta')
  await late.getByRole('button', { name: /Alpha advances/ }).click()
  await expect(page.getByRole('region', { name: 'Late matches' })).toHaveCount(0)
})

test('a captain reports a score from a private link and the organizer approves it', async ({ page, browser }) => {
  await signIn(page)
  await page.getByLabel('New tournament').fill('Captain Cup')
  await page.getByRole('button', { name: 'Create' }).click()
  await page.getByLabel(/Add teams/).fill('Alpha\nBravo\nCharlie\nDelta')
  await page.getByRole('button', { name: 'Add to roster' }).click()
  await page.getByRole('button', { name: 'Start tournament' }).click()
  await expect(page.getByRole('tab', { name: 'Teams' })).toBeVisible()
  await page.getByRole('tab', { name: 'Teams' }).click()
  await page.getByRole('button', { name: /Alpha/ }).first().click()
  await page.getByRole('button', { name: 'Create captain link' }).click()
  const link = (await page.getByRole('dialog').locator('code').innerText()).trim()
  await page.getByRole('dialog').getByRole('button', { name: 'Close' }).click()

  const captain = await browser.newPage()
  await captain.goto(link)
  await expect(captain.getByRole('heading', { name: 'Alpha' })).toBeVisible()
  await expect(captain.getByText('Delta', { exact: false }).first()).toBeVisible()
  await captain.getByLabel('Alpha score').fill('3')
  await captain.getByLabel('Delta score').fill('1')
  await captain.getByRole('button', { name: 'Send score' }).click()
  await expect(captain.getByRole('status')).toContainText('Sent')
  await captain.close()

  await page.getByRole('tab', { name: 'Matches' }).click()
  const reports = page.getByRole('region', { name: 'Reported scores' })
  await expect(reports).toContainText('Alpha reports')
  await reports.getByRole('button', { name: 'Approve' }).click()
  await expect(page.getByRole('region', { name: 'Reported scores' })).toHaveCount(0)
  await page.getByRole('tab', { name: 'Bracket' }).click()
  await expect(page.getByRole('button', { name: 'Edit result for Alpha versus Delta' })).toContainText('3')
})

test('the ladder adds results across tournaments', async ({ page }) => {
  await signIn(page)
  for (const name of ['Ladder One', 'Ladder Two']) {
    await page.goto('/')
    await page.getByLabel('New tournament').fill(name)
    await page.getByRole('button', { name: 'Create' }).click()
    await page.getByLabel(/Add teams/).fill('Ladybug\nOther' + name.slice(-3))
    await page.getByRole('button', { name: 'Add to roster' }).click()
    await page.getByRole('button', { name: 'Start tournament' }).click()
    await record(page, `Edit result for Ladybug versus Other${name.slice(-3)}`, '2', '1')
  }
  await page.getByRole('navigation', { name: 'Primary' }).getByRole('link', { name: 'Ladder' }).click()
  const row = page.getByRole('row', { name: /Ladybug/ })
  await expect(row).toContainText('26')
})
