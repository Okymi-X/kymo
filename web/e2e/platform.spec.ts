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
