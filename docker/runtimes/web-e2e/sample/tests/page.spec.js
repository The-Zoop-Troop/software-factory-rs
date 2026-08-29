import { test, expect } from '@playwright/test';

test('renders a page', async ({ page }) => {
  await page.setContent('<h1 id="t">hello rig</h1>');
  await expect(page.locator('#t')).toHaveText('hello rig');
});
