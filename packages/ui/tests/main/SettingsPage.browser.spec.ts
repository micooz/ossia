import { expect, test, type Locator } from '@playwright/test';

import { mockManagement } from './management.ts';

test('startup behavior is saved alongside other preferences', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  const startup = page.getByRole('group', { name: '首次启动' });
  await expect(startup.getByRole('button', { name: '打开管理窗口' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await startup.getByRole('button', { name: '最小化到托盘' }).click();
  await expect(startup.getByRole('button', { name: '最小化到托盘' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await page.getByRole('group', { name: '颜色主题' }).getByRole('button', { name: '浅色' }).click();
  await expect(startup.getByRole('button', { name: '最小化到托盘' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
});

test('external language changes update management settings', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await page.evaluate(() =>
    (window as Window & { emitLanguage?: (language: string) => void }).emitLanguage?.('en-US'),
  );
  await expect(page.getByLabel('Language')).toHaveValue('en-US');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en-US');
  await page.getByRole('group', { name: 'Theme' }).getByRole('button', { name: 'Light' }).click();
  await expect(page.getByLabel('Language')).toHaveValue('en-US');
});

test('management descriptions wrap while controls stay on one row', async ({ page }, testInfo) => {
  await mockManagement(page, { omrEnabled: true });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  for (const language of ['zh-CN', 'en-US']) {
    await page.locator('.settings-select').selectOption(language);
    for (const width of [760, 860, 1080]) {
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
      const keyRow = page.locator('.github-token-row').first();
      const [copyLeft, controlLeft] = await keyRow.evaluate((row) => {
        const copy = row.querySelector('.setting-copy')?.getBoundingClientRect();
        const control = row.querySelector('.github-token-control')?.getBoundingClientRect();
        return [copy?.left ?? 0, control?.left ?? 0];
      });
      expect(controlLeft).toBeGreaterThan(copyLeft + 100);
      expect(
        await page
          .locator('.page-content')
          .evaluate((element) => element.scrollWidth <= element.clientWidth),
      ).toBe(true);
    }
  }
  await page.screenshot({ path: testInfo.outputPath('settings-wrapped-english.png') });
});

test('LAN access requires confirmation and shows the restarted service address', async ({
  page,
}) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '服务未连接' }).click();
  const service = page.getByRole('dialog', { name: '本地服务' });
  await service.getByRole('button', { name: '启动服务' }).click();
  await service.getByRole('button', { name: '关闭', exact: true }).click();
  await page.getByRole('button', { name: '常规', exact: true }).click();

  const lan = page.getByRole('switch', { name: '局域网访问' });
  await expect(lan).not.toBeChecked();
  await lan.click();
  const confirm = page.getByRole('alertdialog', { name: '开启局域网访问？' });
  await expect(confirm).toHaveText(/更改此设置会重启本地服务，正在执行的编辑任务会被取消。/);
  await expect(confirm).not.toContainText('服务没有用户身份验证');
  await confirm.getByRole('button', { name: '开启' }).click();
  await expect(lan).toBeChecked();
  await expect(page.getByRole('region', { name: '行为' }).locator('code')).toHaveText(
    'http://192.168.1.20:3212',
  );

  await lan.click();
  const disable = page.getByRole('alertdialog', { name: '关闭局域网访问？' });
  await expect(disable).toContainText(
    '其他局域网设备将无法再打开乐谱。更改此设置会重启本地服务，正在执行的编辑任务会被取消。',
  );
  await disable.getByRole('button', { name: '取消' }).click();
  await expect(lan).toBeChecked();
});

test('score recognition settings save a masked key and survive ordinary preference saves', async ({
  page,
}) => {
  await mockManagement(page, { omrService: 'ossia' });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '常规', exact: true }).click();

  const omr = page.locator('.settings-group').filter({ hasText: '乐谱识别' });
  await expect(omr.locator('.settings-row')).toHaveCount(1);
  await expect(omr.getByRole('group', { name: '服务提供方' })).toHaveCount(0);
  await expect(omr.getByLabel('访问密钥')).toHaveCount(0);
  const enabled = omr.getByRole('switch', { name: '使用在线识别' });
  await expect(enabled).toHaveAttribute('aria-checked', 'false');
  await enabled.click();
  await expect(enabled).toHaveAttribute('aria-checked', 'true');
  await enabled.click();
  await expect(enabled).toHaveAttribute('aria-checked', 'false');
  await enabled.click();
  await expect(enabled).toHaveAttribute('aria-checked', 'true');
  await expect(omr.getByRole('group', { name: '服务提供方' })).toBeVisible();
  await expect(omr.getByLabel('服务地址')).toHaveCount(0);
  await expect(omr.getByLabel('访问密钥')).toHaveAttribute('placeholder', '访问密钥');
  const center = (locator: Locator) =>
    locator.evaluate((element) => {
      const box = element.getBoundingClientRect();
      return box.top + box.height / 2;
    });
  const keyCenter = await center(omr.getByLabel('访问密钥'));
  const saveCenter = await center(omr.getByRole('button', { name: '保存', exact: true }));
  expect(Math.abs(keyCenter - saveCenter)).toBeLessThan(2);
  await omr.getByLabel('访问密钥').fill('ossia-token');
  await omr.getByRole('button', { name: '保存', exact: true }).click();

  await expect(omr.getByLabel('访问密钥')).toHaveCount(0);
  await expect(omr.locator('code').last()).toHaveText('••••••••oken');
  await expect(omr.getByRole('button', { name: '移除' })).toBeVisible();
  await expect(page.getByText('乐谱识别设置已保存')).toHaveCount(0);

  await page
    .getByRole('group', { name: '颜色主题' })
    .getByRole('button', { name: '浅色', exact: true })
    .click();
  await expect(omr.locator('code').last()).toHaveText('••••••••oken');

  await omr.getByRole('button', { name: '自定义' }).click();
  await expect(omr.locator('.settings-row')).toHaveCount(4);
  await expect(omr.getByLabel('服务地址')).toBeVisible();
  const omrSaves = () =>
    page.evaluate(
      () =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.filter(
          (command) => command === 'management_omr_settings_set',
        ).length ?? 0,
    );
  const address = omr.getByLabel('服务地址');
  await address.fill('https://omr.example.com/omr');
  const savesBeforeBlur = await omrSaves();
  await address.press('Tab');
  await expect.poll(omrSaves).toBeGreaterThan(savesBeforeBlur);
  await omr.getByLabel('访问密钥').fill('custom1234');
  await omr.getByRole('button', { name: '保存', exact: true }).click();
  await expect(omr.locator('code').last()).toHaveText('••••••••1234');
  await expect(omr.getByLabel('服务地址')).toHaveValue('https://omr.example.com/omr');

  await enabled.click();
  await expect(enabled).toHaveAttribute('aria-checked', 'false');
  await expect(omr.locator('.settings-row')).toHaveCount(1);
  await expect(omr.getByLabel('服务地址')).toHaveCount(0);
  await expect(omr.locator('code')).toHaveCount(0);
  await enabled.click();
  await expect(enabled).toHaveAttribute('aria-checked', 'true');
  await expect(omr.getByLabel('服务地址')).toHaveValue('https://omr.example.com/omr');
  await expect(omr.locator('code').last()).toHaveText('••••••••1234');

  await omr.getByRole('button', { name: '移除' }).click();
  await expect(omr.locator('code')).toHaveCount(0);
  await expect(omr.getByLabel('访问密钥')).toBeVisible();
  await expect(omr.getByLabel('服务地址')).toHaveValue('https://omr.example.com/omr');
  await expect(page.getByText('访问密钥已清除')).toHaveCount(0);

  await address.fill('');
  const savesBeforeClear = await omrSaves();
  await address.press('Tab');
  await expect.poll(omrSaves).toBeGreaterThan(savesBeforeClear);
  await expect(address).toHaveValue('');
  await expect(omr.getByText('请填写服务器地址。')).toHaveCount(0);

  await omr.getByRole('button', { name: 'Ossia OMR' }).click();
  await expect(omr.getByLabel('服务地址')).toHaveCount(0);
  await expect(omr.locator('code').last()).toHaveText('••••••••oken');
});

test('new recognition settings default to custom service', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  const omr = page.locator('.settings-group').filter({ hasText: '乐谱识别' });
  const enabled = omr.getByRole('switch', { name: '使用在线识别' });
  await expect(enabled).not.toBeChecked();
  await enabled.click();
  await expect(enabled).toBeChecked();
  await expect(omr.getByRole('button', { name: '自定义' })).toHaveAttribute('aria-pressed', 'true');
  await expect(omr.getByLabel('服务地址')).toHaveValue('');
  await expect(omr.getByLabel('访问密钥')).toHaveValue('');
});
