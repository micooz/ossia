import { expect, test } from '@playwright/test';

import type { UpdateSnapshot } from '../../src/generated/protocol.ts';
import { mockManagement } from './management.ts';

const found: UpdateSnapshot = {
  state: 'found',
  isDebugBuild: false,
  current: 'v0.1.0',
  source: 'ossia',
  channel: 'stable',
  latest: '0.2.0',
  error: null,
  done: null,
  total: null,
  staged: false,
  notes: {
    from: '0.1.0',
    to: '0.2.0',
    maintenanceOnly: false,
    releases: [
      {
        version: '0.2.0',
        body: [
          '<!-- lang:zh -->',
          '## 新功能',
          '- 支持新的更新流程 [说明](https://example.com/notes)',
          '- ![截图](/tmp/screenshot.png)',
          '<script>window.bad = true</script>',
          '### 安装说明',
          '- ignored',
        ].join('\n'),
      },
    ],
  },
};

test('download failures remain visible and can be retried', async ({ page }) => {
  await mockManagement(page, {
    updateState: { ...found, state: 'error', error: 'download: HTTP 403' },
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await expect(page.getByText('download: HTTP 403', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '更新', exact: true })).toBeEnabled();
});

test('About saves the update source and GitHub token', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await expect(page.locator('.about-page').getByRole('heading', { name: '更新' })).toHaveCount(0);
  await expect(page.locator('.about-page').getByText('数据目录')).toHaveCount(0);
  const source = page.getByRole('group', { name: '更新源' });
  const tokenRow = page.locator('.update-token-row');
  const channel = page.getByRole('group', { name: '更新通道' });
  await expect(tokenRow).toHaveCount(0);
  await expect(page.getByText('Ossia 源为中国大陆优化；GitHub 源全球可用。')).toBeVisible();
  await expect(source.getByRole('button', { name: 'Ossia' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await source.getByRole('button', { name: 'GitHub' }).click();
  await expect(source.getByRole('button', { name: 'GitHub' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(tokenRow).toBeVisible();
  expect((await source.boundingBox())!.y).toBeLessThan((await tokenRow.boundingBox())!.y);
  expect((await tokenRow.boundingBox())!.y).toBeLessThan((await channel.boundingBox())!.y);
  await page.getByLabel('GitHub Token').fill('github_pat_test');
  await page.locator('.update-token-row').getByRole('button', { name: '保存' }).click();
  await expect(page.getByText('••••••••test')).toBeVisible();
  await source.getByRole('button', { name: 'Ossia' }).click();
  await expect(tokenRow).toHaveCount(0);
  await source.getByRole('button', { name: 'GitHub' }).click();
  await expect(page.getByText('••••••••test')).toBeVisible();
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await expect(page.getByRole('heading', { name: '常规', exact: true })).toBeVisible();
  await expect(page.getByLabel('GitHub Token')).toHaveCount(0);
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await expect(source.getByRole('button', { name: 'GitHub' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByText('••••••••test')).toBeVisible();
  await page.locator('.update-token-row').getByRole('button', { name: '移除' }).click();
  await expect(page.getByLabel('GitHub Token')).toBeVisible();
});

test('update action, channel, and automatic download use saved preferences', async ({ page }) => {
  await mockManagement(page, { updateState: found });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: '发现新版本 0.2.0' })).toBeAttached();
  const update = page.getByRole('button', { name: '更新', exact: true });
  await expect(update).toHaveCSS('gap', '6px');
  await update.click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.includes(
          'update_apply',
        ),
      ),
    )
    .toBe(true);

  const autoDownload = page.getByRole('switch', { name: '自动下载更新' });
  await expect(autoDownload).toBeChecked();
  await autoDownload.click();
  await expect(autoDownload).not.toBeChecked();
  await expect(page.getByRole('button', { name: '跟随构建' })).toHaveCount(0);
  await page.getByRole('button', { name: '测试版' }).click();
  await expect(page.getByRole('button', { name: '测试版' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as Window & { managementCommands?: string[] }).managementCommands?.filter(
            (command) => command === 'management_preferences_change',
          ).length,
      ),
    )
    .toBeGreaterThanOrEqual(2);
});

test('returning to About does not replay a completed latest notice', async ({ page }) => {
  await mockManagement(page, {
    updateState: {
      ...found,
      state: 'latest',
      latest: '0.1.0',
      notes: null,
    },
  });
  await page.goto('/main.html');
  const action = page.locator('.update-version-actions .text-button');

  await page.getByRole('button', { name: '关于', exact: true }).click();
  await page.waitForTimeout(100);
  await expect(action).toHaveText('检查更新');

  await page.getByRole('button', { name: '常规', exact: true }).click();
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await page.waitForTimeout(100);
  await expect(action).toHaveText('检查更新');
});

test('release notes are localized, sanitized, and open HTTPS links externally', async ({
  page,
}) => {
  await mockManagement(page, { updateState: found, githubTokenMasked: '••••••••test' });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await page.locator('.update-notes-row').getByRole('button', { name: '打开' }).click();
  const dialog = page.getByRole('dialog', { name: 'v0.2.0 更新内容' });
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('.update-notes-header')).toContainText('v0.1.0 → v0.2.0');
  await expect(dialog.locator('.update-notes-range')).toHaveText('v0.1.0 → v0.2.0');
  await expect(dialog.getByRole('heading', { name: '新功能' })).toBeVisible();
  await expect(dialog.getByText('支持新的更新流程')).toBeVisible();
  await expect(dialog.getByText('截图')).toBeVisible();
  await expect(dialog.locator('script')).toHaveCount(0);
  await expect(dialog.locator('img')).toHaveCount(0);
  await expect(dialog.getByText('ignored')).toHaveCount(0);
  await dialog.getByRole('link', { name: '说明' }).click();
  await expect
    .poll(() =>
      page.evaluate(() => (window as Window & { openedLinks?: string[] }).openedLinks ?? []),
    )
    .toContain('https://example.com/notes');
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toHaveCount(0);
});

test('release notes work without native dialog support', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { value: undefined });
    Object.defineProperty(Array.prototype, 'at', { value: undefined, configurable: true });
    Object.defineProperty(String.prototype, 'at', { value: undefined, configurable: true });
  });
  await mockManagement(page, { updateState: found });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  const trigger = page.locator('.update-notes-row').getByRole('button', { name: '打开' });
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: 'v0.2.0 更新内容' });
  await expect(dialog.getByText('支持新的更新流程')).toBeVisible();
  await expect
    .poll(async () =>
      dialog.evaluate((element) => {
        const rect = element.getBoundingClientRect();
        return Math.abs(rect.top + rect.height / 2 - window.innerHeight / 2);
      }),
    )
    .toBeLessThan(2);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

test('English release notes hide installation notes written in Chinese', async ({ page }) => {
  await mockManagement(page, {
    updateState: {
      ...found,
      notes: {
        from: '0.1.0',
        to: '0.2.0',
        maintenanceOnly: false,
        releases: [
          {
            version: '0.2.0',
            body: [
              '## Fixes',
              '- Fixed the updater.',
              '',
              '### 安装说明',
              '- Should not appear',
              '',
              '<!-- lang:zh -->',
              '## 修复',
              '- 中文修复',
              '',
              '### 安装说明',
              '- 不应显示',
            ].join('\n'),
          },
        ],
      },
    },
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await page.locator('.settings-select').selectOption('en-US');
  await page.getByRole('button', { name: 'About', exact: true }).click();
  await page.locator('.update-notes-row').getByRole('button', { name: 'Open' }).click();
  const dialog = page.getByRole('dialog', { name: 'v0.2.0 release notes' });

  await expect(dialog.getByText('Fixed the updater.')).toBeVisible();
  await expect(dialog.getByText('安装说明')).toHaveCount(0);
  await expect(dialog.getByText('Should not appear')).toHaveCount(0);
});

test('release notes merge categories and keep version badges inside entries', async ({
  page,
}, testInfo) => {
  await mockManagement(page, {
    updateState: {
      ...found,
      notes: {
        from: '0.1.0',
        to: '0.2.0',
        maintenanceOnly: false,
        releases: [
          {
            version: '0.2.0',
            body: '<!-- lang:zh -->\n## 错误修复\n- 新的修复\n- 另一项修复',
          },
          {
            version: '0.1.9',
            body: '<!-- lang:zh -->\n## 错误修复\n- 旧的修复\n\n## 新功能\n- 新的流程',
          },
        ],
      },
    },
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await page.locator('.update-notes-row').getByRole('button', { name: '打开' }).click();
  const dialog = page.getByRole('dialog', { name: 'v0.2.0 更新内容' });
  await expect(dialog).toHaveCSS('opacity', '1');

  await expect(dialog.getByRole('heading', { name: '错误修复' })).toHaveCount(1);
  const fixes = dialog.locator('.release-notes-category').filter({ hasText: '错误修复' });
  await expect(fixes.locator('li')).toHaveCount(3);
  await expect(fixes.locator('li').nth(0).locator('.release-version')).toHaveText('v0.2.0');
  await expect(fixes.locator('li').nth(1).locator('.release-version')).toHaveText('v0.2.0');
  await expect(fixes.locator('li').nth(2).locator('.release-version')).toHaveText('v0.1.9');
  await expect(dialog.locator('.update-notes-entry')).toHaveCount(0);

  await page.screenshot({ path: testInfo.outputPath('release-notes-dialog.png') });
});

for (const fallback of [false, true]) {
  test(`release notes keep the header and footer fixed while the body scrolls (fallback: ${fallback})`, async ({
    page,
  }, testInfo) => {
    if (fallback) {
      await page.addInitScript(() => {
        Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { value: undefined });
      });
    }
    const newest = '0.2.12';
    const releases = Array.from({ length: 12 }, (_, index) => ({
      version: `0.2.${12 - index}`,
      body: [
        '<!-- lang:zh -->',
        '## 错误修复',
        ...Array.from({ length: 4 }, (_, item) => `- 第 ${index + 1} 个版本的修复 ${item + 1}`),
      ].join('\n'),
    }));
    await mockManagement(page, {
      updateState: {
        ...found,
        notes: {
          from: '0.1.0',
          to: newest,
          maintenanceOnly: false,
          releases,
        },
      },
    });
    await page.goto('/main.html');
    await page.getByRole('button', { name: '关于', exact: true }).click();
    await page.locator('.update-notes-row').getByRole('button', { name: '打开' }).click();
    const dialog = page.getByRole('dialog', { name: 'v0.2.12 更新内容' });
    await expect(dialog).toHaveCSS('opacity', '1');

    const header = dialog.locator('.update-notes-header');
    const content = dialog.locator('.update-notes-content');
    const footer = dialog.locator('.update-notes-footer');
    const bounds = await dialog.boundingBox();
    const viewport = page.viewportSize();
    expect(bounds).not.toBeNull();
    expect(viewport).not.toBeNull();
    expect(bounds!.y).toBeGreaterThanOrEqual(0);
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(viewport!.height);
    expect(await content.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(
      true,
    );
    const before = [await header.boundingBox(), await footer.boundingBox()];
    await content.evaluate((element) => {
      element.scrollTop = element.scrollHeight;
    });
    expect([await header.boundingBox(), await footer.boundingBox()]).toEqual(before);

    await page.screenshot({ path: testInfo.outputPath('release-notes-scroll.png') });
  });
}

test('the page header update action starts installation directly', async ({ page }) => {
  await mockManagement(page, { updateState: found });
  await page.goto('/main.html');
  const header = page.getByRole('button', { name: '更新', exact: true }).first();
  await expect(header).toBeVisible();
  const updateBox = await header.boundingBox();
  const serviceBox = await page.locator('.header-actions .service-badge').boundingBox();
  expect(updateBox).not.toBeNull();
  expect(serviceBox).not.toBeNull();
  expect(updateBox!.x).toBeLessThan(serviceBox!.x);
  await header.click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.includes(
          'update_apply',
        ),
      ),
    )
    .toBe(true);
});

test('development builds can set the update channel', async ({ page }) => {
  await mockManagement(page, {
    githubTokenMasked: '••••••••test',
    updateState: {
      ...found,
      isDebugBuild: true,
    },
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  const channel = page.getByRole('group', { name: '更新通道' });
  await expect(channel.getByRole('button', { name: '稳定版' })).toBeEnabled();
  await channel.getByRole('button', { name: '测试版' }).click();
  await expect(channel.getByRole('button', { name: '测试版' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  const autoDownload = page.getByRole('switch', { name: '自动下载更新' });
  await expect(autoDownload).toBeChecked();
  await expect(autoDownload).toBeEnabled();
  await expect(page.getByRole('button', { name: '下载验证' })).toBeEnabled();
});

test('development builds show verified downloads without an install action', async ({ page }) => {
  await mockManagement(page, {
    updateState: { ...found, isDebugBuild: true, state: 'ready', staged: true },
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await expect(page.getByRole('button', { name: '下载并校验完成' })).toBeDisabled();
  await expect(
    page.getByText('开发版 0.1.0 可检查、下载和校验最新发布版，不会安装或重启。'),
  ).toBeVisible();
});

test('development builds open release notes on demand', async ({ page }) => {
  await mockManagement(page, {
    updateState: {
      ...found,
      state: 'source',
      isDebugBuild: true,
      latest: null,
      notes: null,
    },
    releaseNotesState: {
      ...found,
      state: 'source',
      isDebugBuild: true,
      latest: null,
    },
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  const open = page.locator('.update-notes-row').getByRole('button', { name: '打开' });
  await expect(open).toBeEnabled();
  await open.click();
  await expect(page.getByRole('dialog', { name: 'v0.2.0 更新内容' })).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.includes(
          'release_notes',
        ),
      ),
    )
    .toBe(true);
});

test('release builds fetch release notes when the snapshot has none', async ({ page }) => {
  await mockManagement(page, {
    updateState: {
      ...found,
      state: 'latest',
      current: '0.2.0',
      latest: '0.2.0',
      notes: null,
    },
    releaseNotesState: found,
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  const open = page.locator('.update-notes-row').getByRole('button', { name: '打开' });
  await expect(open).toBeEnabled();
  await open.click();
  await expect(page.getByRole('dialog', { name: 'v0.2.0 更新内容' })).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.includes(
          'release_notes',
        ),
      ),
    )
    .toBe(true);
});

test('about rows keep controls beside their labels at common window widths', async ({ page }) => {
  await mockManagement(page, { updateState: found });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  for (const width of [760, 860, 1080]) {
    await page.setViewportSize({ width, height: 720 });
    for (const row of await page.locator('.about-page .settings-row').all()) {
      const label = await row.locator('.setting-label').first().boundingBox();
      const control = await row
        .locator('.update-version-actions, .secondary-button, .switch, .segmented, .text-button')
        .first()
        .boundingBox();
      expect(control!.y).toBeLessThan(label!.y + label!.height);
      expect(control!.y + control!.height).toBeGreaterThan(label!.y);
    }
    expect(
      await page
        .locator('.page-content')
        .evaluate((element) => element.scrollWidth <= element.clientWidth),
    ).toBe(true);
  }
});

test('cancels a download from About and enables changing its source', async ({ page }) => {
  await mockManagement(page, {
    updateState: { ...found, state: 'downloading', done: 29, total: 100 },
  });
  await page.goto('/main.html');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  const source = page.getByRole('group', { name: '更新源' });
  await expect(source.getByRole('button', { name: 'GitHub' })).toBeDisabled();
  await expect(page.getByRole('button', { name: /正在下载… \d+%/ })).toBeDisabled();
  await page.getByRole('button', { name: '取消', exact: true }).click();
  await expect(source.getByRole('button', { name: 'GitHub' })).toBeEnabled();
  await source.getByRole('button', { name: 'GitHub' }).click();
  await expect(source.getByRole('button', { name: 'GitHub' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
});

test('cancels a download from the title bar', async ({ page }) => {
  await mockManagement(page, {
    updateState: { ...found, state: 'downloading', done: 29, total: 100 },
  });
  await page.goto('/main.html');
  const cancel = page.getByRole('button', { name: '取消下载更新' });
  await expect(cancel).toBeVisible();
  await cancel.focus();
  await page.keyboard.press('Enter');
  await expect(cancel).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.includes(
          'update_cancel',
        ),
      ),
    )
    .toBe(true);
});

for (const state of ['checking', 'installing'] as const) {
  test(`does not offer cancellation while ${state}`, async ({ page }) => {
    await mockManagement(page, { updateState: { ...found, state } });
    await page.goto('/main.html');
    await expect(page.getByRole('button', { name: '取消下载更新' })).toHaveCount(0);
    await page.getByRole('button', { name: '关于', exact: true }).click();
    await expect(page.getByRole('button', { name: '取消', exact: true })).toHaveCount(0);
  });
}

test('development builds show download progress and cancellation in the header', async ({
  page,
}) => {
  await mockManagement(page, {
    updateState: { ...found, isDebugBuild: true, state: 'downloading', done: 30, total: 100 },
  });
  await page.goto('/main.html');
  await expect(page.getByRole('button', { name: /正在下载… 30%/ })).toBeDisabled();
  await page.getByRole('button', { name: '取消下载更新' }).click();
  await expect(page.getByRole('button', { name: '取消下载更新' })).toHaveCount(0);
});

test('development builds show a verified download without an install action in the header', async ({
  page,
}) => {
  await mockManagement(page, {
    updateState: { ...found, isDebugBuild: true, state: 'ready', staged: true },
  });
  await page.goto('/main.html');
  await expect(page.getByRole('button', { name: '下载并校验完成' })).toBeDisabled();
});
