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
  await page.getByRole('button', { name: 'Plan' }).click();
  await expect(page.getByText(/Plan queued as/)).toBeVisible();
  await expect(page.getByRole('heading', { name: /Planning/ })).toBeVisible();
  await expect(page.getByText(/claimed by worker-1|landed on main/).first()).toBeVisible();
  await expect(page.getByText('live', { exact: true })).toBeVisible();
  await page.getByLabel('Your answer or resolution').fill('done it');
  await page.getByRole('button', { name: 'Resolve' }).click();
  await expect(page.getByText(/Resolved/)).toBeVisible();
  await page.getByRole('button', { name: 'Stop' }).first().click();
  await expect(page.getByText(/Stopped/)).toBeVisible();
});

test('a bad token is explained', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Token').fill('wrong');
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.getByRole('alert')).toContainText('Not signed in');
});
