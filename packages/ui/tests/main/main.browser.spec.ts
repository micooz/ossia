import { expect, test } from '@playwright/test';

import { mockManagement } from './management.ts';

test('no ticket explains how to open; browser management cannot invoke native commands', async ({
  page,
}) => {
  await page.goto('/score.html');
  await expect(page.getByRole('alert')).toContainText('ossia_open');
  await page.goto('/main.html');
  await expect(page.getByRole('alert')).toContainText('桌面应用');
  await page.getByRole('button', { name: '服务未连接' }).click();
  await expect(page.getByRole('dialog', { name: '本地服务' })).toBeVisible();
  await expect(page.getByRole('button', { name: '启动服务' })).toBeDisabled();
  await expect(page.getByRole('button', { name: '启动服务' })).toHaveCSS('cursor', 'not-allowed');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Agent', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.locator('[data-tauri-drag-region="deep"]')).toHaveCount(2);
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await expect(page.getByText('将 Ossia 接入到受支持的 Agent 中')).toBeVisible();
  await expect(page.getByRole('button', { name: '全部' })).toHaveCount(0);
  await page.getByRole('button', { name: '常规' }).click();
  await expect(page.getByRole('heading', { name: '常规' })).toBeVisible();
  await expect(page.getByLabel('语言')).toBeDisabled();
  await expect(page.getByLabel('语言')).toHaveCSS('cursor', 'not-allowed');
  await page.emulateMedia({ colorScheme: 'dark' });
  expect(
    await page.evaluate(() =>
      getComputedStyle(globalThis.document.documentElement).getPropertyValue('--page').trim(),
    ),
  ).toBe('#1c1c20');
});

test.describe('English management shell', () => {
  test.use({ locale: 'en-US' });

  test('uses the selected system language without native settings', async ({ page }) => {
    await page.goto('/main.html');
    await expect(page.getByText('Local service connected')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Service not connected' })).toBeVisible();
    await expect(page.getByRole('alert')).toContainText('Open the management window');
    await page.getByRole('button', { name: 'General' }).click();
    await expect(page.getByRole('heading', { name: 'General' })).toBeVisible();
    await expect(page.getByLabel('Language')).toHaveValue('en-US');
  });
});

test('management restores the last panel after reload and reopening the window', async ({
  context,
  page,
}) => {
  await page.goto('/main.html');
  await expect(page.getByRole('button', { name: 'Agent', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await page.reload();
  await expect(page.getByRole('heading', { name: '常规', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await page.close();
  const reopened = await context.newPage();
  await reopened.goto('/main.html');
  await expect(reopened.getByRole('button', { name: '关于', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await reopened.getByRole('button', { name: 'Agent', exact: true }).click();
  await reopened.reload();
  await expect(reopened.getByRole('heading', { name: 'Agent 接入', exact: true })).toBeVisible();
});

test('storage settings show usage and run cleanup for the connected runtime', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '服务未连接' }).click();
  const dialog = page.getByRole('dialog', { name: '本地服务' });
  await dialog.getByRole('button', { name: '启动服务' }).click();
  await expect(dialog.getByRole('button', { name: '停止服务', exact: true })).toBeEnabled();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await expect(page.getByRole('heading', { name: '存储' })).toBeVisible();
  await expect(page.getByText('95 MiB / 1.0 GiB')).toBeVisible();
  await expect(
    page.getByText('42 条任务 · 18 条渲染记录 / 核心 1.3 GiB · 缓存 95 MiB · 临时 0 B'),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: '1 GB' })).toBeVisible();
  await expect(page.getByRole('button', { name: '3 GB' })).toBeVisible();
  await expect(page.getByRole('button', { name: '10 GB' })).toBeVisible();
  const autoCleanup = page.getByRole('switch', { name: '自动清理' });
  await expect(autoCleanup).toBeChecked();
  await expect(autoCleanup).toHaveCSS('width', '38px');
  await expect(autoCleanup).toHaveCSS('height', '22px');
  await expect(autoCleanup).toHaveCSS('border-top-width', '0px');
  await autoCleanup.click();
  await expect(autoCleanup).not.toBeChecked();
  await expect(page.getByText('自动清理已关闭，仍可手动清理。')).toBeVisible();
  await expect(page.getByRole('button', { name: '立即清理' })).toBeEnabled();
  await autoCleanup.click();
  await expect(autoCleanup).toBeChecked();
  const cleanup = page.getByRole('button', { name: '立即清理' });
  await expect(cleanup).toHaveCSS('cursor', 'pointer');
  const cleanupBackground = await cleanup.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  await cleanup.hover();
  expect(await cleanup.evaluate((element) => getComputedStyle(element).backgroundColor)).not.toBe(
    cleanupBackground,
  );
  await page.mouse.down();
  await expect
    .poll(() => cleanup.evaluate((element) => getComputedStyle(element).transform))
    .toContain('matrix');
  await page.mouse.move(0, 0);
  await page.mouse.up();
  await cleanup.click();
  await expect(page.getByText('已释放 2.4 MiB；移除 2 条任务和 1 条渲染记录。')).toBeVisible();
  await page.getByRole('button', { name: '512 MiB' }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as Window & { managementCommands?: string[] }).managementCommands?.filter(
            (command) => command === 'management_storage_cleanup',
          ).length,
      ),
    )
    .toBeGreaterThan(1);
});

test('management shows details and target config paths only for connected agents', async ({
  page,
}, testInfo) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await expect(page.getByRole('button', { name: '全部' })).toHaveCount(0);
  await expect(page.locator('.agent-detail')).toHaveCount(1);
  await expect(page.getByText('Agent 版本')).toBeVisible();
  await expect(page.getByText('Profile', { exact: true })).toHaveCount(0);
  await expect(page.getByText('点击以下 Agent 接入或取消接入')).toBeVisible();
  await expect(page.locator('.agent-detail dt')).toHaveText([
    'Agent 版本',
    '接入方式',
    '插件位置',
    '接入目标',
  ]);
  for (const width of [800, 1600]) {
    await page.setViewportSize({ width, height: 900 });
    const content = await page.locator('.page-content').boundingBox();
    const panel = await page.locator('.agent-list').boundingBox();
    expect(panel!.width / content!.width).toBeGreaterThan(0.85);
  }
  await expect(page.getByText('接入目标')).toBeVisible();
  await expect(page.getByText('/Users/example/.codex/config.toml')).toBeVisible();
  await expect(page.getByText('/Users/example/.dsh/profiles/desktop/package.json')).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('agents-wide.png') });
  const codex = page.getByRole('button', { name: '移除 Codex 插件' });
  const background = await codex.evaluate((button) => getComputedStyle(button).backgroundColor);
  await codex.hover();
  expect(await codex.evaluate((button) => getComputedStyle(button).backgroundColor)).toBe(
    background,
  );
  await codex.click();
  await expect(codex).toHaveAttribute('aria-busy', 'true');
  await expect(codex.locator('.spinner')).toBeVisible();
  await expect(page.getByRole('button', { name: '安装 Codex 插件' })).toBeEnabled();
  await page.getByRole('button', { name: '安装 Codex 插件' }).click();
  await expect(page.getByRole('button', { name: '移除 Codex 插件' })).toBeEnabled();

  await page.getByRole('button', { name: '安装 DeepSeek Harness 插件' }).click();
  await expect(page.locator('.agent-detail-head strong')).toHaveText(['Codex', 'DeepSeek Harness']);
  await page.getByRole('button', { name: '移除 Codex 插件' }).click();
  await expect(page.getByRole('button', { name: '安装 Codex 插件' })).toBeEnabled();
  await page.getByRole('button', { name: '安装 Codex 插件' }).click();
  await expect(page.locator('.agent-detail-head strong')).toHaveText(['DeepSeek Harness', 'Codex']);
  await expect(page.locator('.agent-detail').first().locator('dd').first()).toHaveText(
    '0.2.2-rc.1 (desktop)',
  );

  await page.getByRole('button', { name: '服务未连接' }).click();
  const dialog = page.getByRole('dialog', { name: '本地服务' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: '关闭', exact: true })).not.toBeFocused();
  await expect(dialog.getByText('LilyPond 工具链')).toBeVisible();
  await expect(dialog.getByRole('button', { name: '刷新' })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: '运行 doctor' })).toHaveCount(0);
  await dialog.getByRole('button', { name: '打开', exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.includes(
          'management_open_data_directory',
        ),
      ),
    )
    .toBe(true);
  await dialog.getByRole('button', { name: '启动服务' }).click();
  await expect(dialog.getByRole('button', { name: '停止服务', exact: true })).toBeEnabled();
  await page.screenshot({ path: testInfo.outputPath('service-dialog.png') });
  await dialog.getByRole('button', { name: '停止服务', exact: true }).click();
  await expect(dialog.getByText('停止会取消当前任务；未提交的源码会保留。')).toBeVisible();
  await dialog.getByRole('button', { name: '确认停止服务' }).click();
  await expect(dialog.getByRole('button', { name: '启动服务' })).toBeEnabled();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: '服务未连接' })).toBeFocused();
  const refreshes = () =>
    page.evaluate(
      () =>
        ((window as Window & { managementCommands?: string[] }).managementCommands ?? []).filter(
          (c) => c === 'management_status',
        ).length,
    );
  const before = await refreshes();
  await page.getByRole('button', { name: '服务未连接' }).click();
  await expect.poll(refreshes).toBeGreaterThan(before);
  await dialog.getByRole('button', { name: '关闭', exact: true }).click();

  for (const width of [860, 1600]) {
    await page.setViewportSize({ width, height: 900 });
    for (const name of ['常规', '关于']) {
      await page.getByRole('button', { name, exact: true }).click();
      if (name === '常规') {
        for (const row of await page.locator('.settings-page .settings-row').all()) {
          const label = await row.locator('.setting-label').boundingBox();
          const controlLocator = row.locator('.segmented, select, .switch');
          if ((await controlLocator.count()) === 0) continue;
          const control = await controlLocator.boundingBox();
          expect(control!.y).toBeLessThan(label!.y + label!.height);
          for (const button of await row.locator('.segment-button').all()) {
            expect(
              await button.evaluate((element) => element.scrollHeight <= element.clientHeight),
            ).toBe(true);
          }
        }
      }
      expect(
        await page
          .locator('.page-content')
          .evaluate((element) => element.scrollWidth <= element.clientWidth),
      ).toBe(true);
      await page.screenshot({ path: testInfo.outputPath(`${name}-${width}.png`) });
    }
  }
  await expect(page.getByText('运行时版本')).toBeHidden();
  await expect(page.getByText('LilyPond 工具链')).toBeHidden();
  await expect(page.getByRole('button', { name: '打开', exact: true })).toBeVisible();
});
