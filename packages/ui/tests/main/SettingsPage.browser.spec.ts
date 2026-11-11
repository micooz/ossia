import { expect, test } from '@playwright/test';

import { mockManagement } from './management.ts';

test('management descriptions wrap while controls stay on one row', async ({ page }, testInfo) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  for (const language of ['zh-CN', 'en-US']) {
    await page.locator('.settings-select').selectOption(language);
    for (const width of [860, 1080]) {
      await page.setViewportSize({ width, height: 720 });
      const note = page.locator('.setting-copy .setting-note').first();
      expect(await note.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(
        true,
      );
      if (width === 860) {
        expect(
          await note.evaluate(
            (element) =>
              element.getBoundingClientRect().height /
              Number.parseFloat(getComputedStyle(element).lineHeight),
          ),
        ).toBeGreaterThan(1);
      }
      const controls = page.getByRole('group', {
        name: language === 'en-US' ? 'Click tray icon' : '点击托盘图标',
      });
      for (const button of await controls.getByRole('button').all()) {
        expect(
          await button.evaluate((element) => element.scrollHeight <= element.clientHeight),
        ).toBe(true);
      }
      expect(
        await page
          .locator('.page-content')
          .evaluate((element) => element.scrollWidth <= element.clientWidth),
      ).toBe(true);
    }
  }
  await page.screenshot({ path: testInfo.outputPath('settings-wrapped-english.png') });
});
