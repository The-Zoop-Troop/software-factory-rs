import { test, expect } from '@playwright/test';

// Against `console serve --fake`: token "fake", rig "toy" with one working epic and one incident.
test('connect, see rigs, open a rig, plan, resolve, stop', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Token').fill('fake');
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.getByText('online')).toBeVisible();
  await page.getByRole('link', { name: /toy/ }).click();
  await expect(page).toHaveURL(/\/rigs\/toy$/);
  await expect(page.getByRole('heading', { name: 'toy' })).toBeVisible();
  await expect(page.getByText('needs you').first()).toBeVisible();
  await page.getByLabel(/Plan — what should/).fill('Add a reverse function with a test and README entry');
  await page.getByRole('button', { name: 'Plan', exact: true }).click();
  await expect(page.getByText(/Plan queued as/)).toBeVisible();
  await expect(page.getByRole('heading', { name: /Planning/ })).toBeVisible();
  await expect(page.getByText(/claimed by worker-1|landed on main/).first()).toBeVisible();
  await expect(page.getByText('live', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(page.getByText(/retry fresh applied/)).toBeVisible();
  await page.getByRole('button', { name: 'Stop' }).first().click();
  await page.getByRole('button', { name: 'Stop the epic' }).click();
  await expect(page.getByText(/Stopped/)).toBeVisible();
});

test('a watch-only token sees a read-only board with reasons', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Token').fill('watcher');
  await page.getByRole('button', { name: 'Connect' }).click();
  await page.getByRole('link', { name: /toy/ }).click();
  await expect(page.getByRole('button', { name: 'Plan', exact: true })).toBeDisabled();
  await expect(page.getByText(/no `plan` scope/).first()).toBeVisible();
});

test('an incident offers evidence and options', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Token').fill('fake');
  await page.getByRole('button', { name: 'Connect' }).click();
  await page.getByRole('link', { name: /toy/ }).click();
  await expect(page.getByText('The branch no longer merges')).toBeVisible();
  await page.getByRole('button', { name: 'Retry with guidance' }).click();
  await page.getByLabel('Your note').fill('keep it POSIX');
  await page.getByRole('button', { name: 'Confirm' }).click();
  await expect(page.getByText(/retry with guidance applied/)).toBeVisible();
});

test('a bad token is explained', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Token').fill('wrong');
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.getByRole('alert')).toContainText('Not signed in');
});
