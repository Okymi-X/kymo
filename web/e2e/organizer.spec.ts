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
