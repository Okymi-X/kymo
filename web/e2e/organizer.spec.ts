import { expect, test, type Page } from '@playwright/test'
import { PASSWORD } from './constants.js'

async function signIn(page: Page) {
  await page.goto('/login')
  await page.getByLabel('Password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Tournaments' })).toBeVisible()
}

async function record(page: Page, match: string, a: string, b: string) {
  await page.getByRole('button', { name: match }).click()
  const scores = page.getByRole('dialog').getByLabel('Score')
  await scores.nth(0).fill(a)
  await scores.nth(1).fill(b)
  await page.getByRole('button', { name: 'Save' }).click()
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
  await expect(page.getByRole('button', { name: 'Primary action' })).toHaveCSS('background-color', 'rgb(170, 34, 0)')

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
  await page.getByRole('button', { name: 'Sign out' }).click()
  await expect(page.getByRole('link', { name: /Sign in|Organizer sign in/ }).first()).toBeVisible()
  const after = await request.get('/api/v1/admin/tokens', { headers: { authorization: `Bearer ${token}` } })
  expect(after.status()).toBe(401)
})
