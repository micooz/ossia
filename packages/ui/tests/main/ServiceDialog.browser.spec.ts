import { expect, test } from '@playwright/test';

import { mockManagement } from './management.ts';

for (const fallback of [false, true]) {
  test(`management dialog animates close and restores focus (fallback: ${fallback})`, async ({
    page,
  }) => {
    await mockManagement(page);
    if (fallback) {
      await page.addInitScript(() => {
        const supports = CSS.supports.bind(CSS);
        CSS.supports = (property: string, value?: string) =>
          property === 'overlay'
            ? false
            : value === undefined
              ? supports(property)
              : supports(property, value);
      });
    }
    await page.goto('/main.html');
    const trigger = page.getByRole('button', { name: '服务未连接' });
    const dialog = page.locator('.service-dialog');
    for (const reducedMotion of ['no-preference', 'reduce'] as const) {
      await page.emulateMedia({ reducedMotion });
      for (const escape of [false, true]) {
        await trigger.click();
        await expect(dialog.getByRole('heading', { name: '本地服务' })).toBeFocused();
        await dialog.evaluate(async (element) => {
          await Promise.allSettled(element.getAnimations().map((animation) => animation.finished));
        });
        if (escape) await page.keyboard.press('Escape');
        else await dialog.getByRole('button', { name: '关闭', exact: true }).click();
        expect(await dialog.evaluate((element) => element.getAnimations().length)).toBeGreaterThan(
          0,
        );
        await expect(dialog).not.toBeVisible();
        await expect(trigger).toBeFocused();
      }
    }
  });
}
