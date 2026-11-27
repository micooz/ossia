import { expect, test } from '@playwright/test';

import { mockManagement } from './management.ts';

test('service dialog can be reopened without native dialog support', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { value: undefined });
  });
  await mockManagement(page);
  await page.goto('/main.html');
  const trigger = page.getByRole('button', { name: '服务未连接' });
  const dialog = page.getByRole('dialog', { name: '本地服务' });
  for (let i = 0; i < 2; i++) {
    await trigger.click();
    await expect(dialog).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(dialog).not.toBeVisible();
    await expect(trigger).toBeFocused();
  }
});

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
    await page.emulateMedia({ reducedMotion: fallback ? 'reduce' : 'no-preference' });
    await trigger.click();
    await expect(dialog.getByRole('heading', { name: '本地服务' })).toBeFocused();
    await dialog.evaluate(async (element) => {
      await Promise.allSettled(element.getAnimations().map((animation) => animation.finished));
      element.addEventListener(
        'transitionrun',
        () => element.setAttribute('data-transition-ran', 'true'),
        {
          once: true,
        },
      );
    });
    if (fallback) await page.keyboard.press('Escape');
    else await dialog.getByRole('button', { name: '关闭', exact: true }).click();
    await expect(dialog).toHaveAttribute('data-transition-ran', 'true');
    await expect(dialog).not.toBeVisible();
    await expect(trigger).toBeFocused();
  });
}

test('opens the service address when the service is running', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '服务未连接' }).click();
  const dialog = page.getByRole('dialog', { name: '本地服务' });
  await dialog.getByRole('button', { name: '打开数据目录' }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () => (window as Window & { managementCommands?: string[] }).managementCommands,
      ),
    )
    .toContain('management_open_data_directory');
  const openAddress = dialog.getByRole('button', { name: '打开服务地址' });
  await expect(dialog.getByText('运行状态')).toBeVisible();
  await expect(dialog.getByText('服务未连接')).toBeVisible();
  await expect(openAddress).toBeDisabled();
  await expect(dialog.getByText('LilyPond 工具链')).toHaveCount(0);
  await dialog.getByRole('button', { name: '启动服务' }).click();
  await expect(dialog.getByText('服务运行中')).toBeVisible();
  await expect(openAddress).toBeEnabled();
  await openAddress.click();
  await expect
    .poll(() => page.evaluate(() => (window as Window & { openedLinks?: string[] }).openedLinks))
    .toEqual(['http://127.0.0.1:3212']);
});
