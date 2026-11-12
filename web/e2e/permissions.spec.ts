import { expect, test, type Page } from '@playwright/test'
import { PASSWORD } from './constants.js'

async function organizer(page: Page) {
  await page.goto('/login')
  await page.getByLabel('Password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible()
}

test('an organizer edits what each role may do, and it sticks', async ({ page }) => {
  await organizer(page)
  await page.getByRole('button', { name: 'Organizer menu' }).click()
  await page.getByRole('menuitem', { name: 'Permissions' }).click()
  await expect(page.getByRole('heading', { name: 'Permissions' })).toBeVisible()

  // Admin and visitor are fixed, and public viewing cannot be switched off for anyone.
  await expect(page.getByLabel('Admin: Manage permissions')).toBeDisabled()
  await expect(page.getByLabel('Admin: Manage permissions')).toBeChecked()
  await expect(page.getByLabel('Visitor: Submit flags')).toBeDisabled()
  await expect(page.getByLabel('Player: View public pages')).toBeDisabled()

  const teams = page.getByLabel('Player: Manage a team')
  await expect(teams).toBeChecked()
  await expect(page.getByText('No unsaved changes.')).toBeVisible()
  await teams.uncheck()
  await expect(page.getByText('1 unsaved change.')).toBeVisible()

  // Discard puts it back.
  await page.getByRole('button', { name: 'Discard' }).click()
  await expect(teams).toBeChecked()
  await expect(page.getByText('No unsaved changes.')).toBeVisible()

  // Save, reload, still off.
  await teams.uncheck()
  await page.getByRole('button', { name: 'Save changes' }).click()
  await expect(page.getByText('No unsaved changes.')).toBeVisible()
  await page.reload()
  await expect(page.getByLabel('Player: Manage a team')).not.toBeChecked()

  // Handing out a powerful permission asks first, and the safe answer is the default.
  const users = page.getByLabel('Monitor: Manage users')
  await users.check()
  await page.getByRole('button', { name: 'Save changes' }).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByText('Hand out powerful permissions?')).toBeVisible()
  await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused()
  await dialog.getByRole('button', { name: 'Cancel' }).click()
  await expect(users).toBeChecked()
  await page.getByRole('button', { name: 'Discard' }).click()
  await expect(users).not.toBeChecked()

  // Reset restores the shipped permissions.
  await page.getByRole('button', { name: 'Reset to defaults' }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Reset', exact: true }).click()
  await expect(page.getByLabel('Player: Manage a team')).toBeChecked()
})

test('a player does not see the permissions page', async ({ page }) => {
  await page.goto('/signin')
  await page.getByRole('tab', { name: 'Create account' }).click().catch(() => undefined)
  await page.goto('/admin/permissions')
  await expect(page.getByRole('heading', { name: 'Permissions' })).toHaveCount(0)
})
