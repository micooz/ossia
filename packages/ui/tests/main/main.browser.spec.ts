import { expect, test } from '@playwright/test';

import { mockManagement, releaseAgentChange } from './management.ts';

test('closed dialogs do not affect the management layout without native dialog styles', async ({
  page,
}) => {
  await page.goto('/main.html');
  await page.addStyleTag({ content: 'dialog { display: block; position: static; }' });
  for (const height of [500, 800]) {
    await page.setViewportSize({ width: 1100, height });
    const sidebarHeight = await page
      .locator('.sidebar')
      .evaluate((element) => element.getBoundingClientRect().height);
    expect(sidebarHeight).toBe(height);
  }
});

test('agent loading failure shows the empty state and an error toast', async ({ page }) => {
  await mockManagement(page, { agentLoadError: 'Agent state unavailable' });
  await page.goto('/main.html');
  await expect(page.locator('.toast-region')).toContainText('Agent state unavailable');
  await expect(page.getByText('正在检测可接入的 Agent…')).toHaveCount(0);
  await expect(page.getByText('未检测到可接入的 Agent', { exact: false })).toBeVisible();
  await expect(page.locator('.agent-list')).not.toContainText('Agent state unavailable');
});

test('agent tooltip animates its entry and exit and respects reduced motion', async ({ page }) => {
  await mockManagement(page, { agentConfigError: 'invalid mcp_config.json' });
  await page.goto('/main.html');
  const agent = page.getByRole('button', { name: /DeepSeek Harness/ });
  await agent.hover();
  const tooltip = page.locator('[data-slot="tooltip-content"]');
  await expect(tooltip).toHaveCSS('animation-name', 'ui-tooltip-enter');
  const opacity = await tooltip.evaluate((element) => {
    const animation = element.getAnimations()[0];
    if (!animation) throw new Error('Tooltip entry animation is missing');
    animation.pause();
    animation.currentTime = 0;
    const start = Number(getComputedStyle(element).opacity);
    animation.currentTime = 75;
    const middle = Number(getComputedStyle(element).opacity);
    animation.finish();
    return { start, middle };
  });
  expect(opacity.start).toBe(0);
  expect(opacity.middle).toBeGreaterThan(0);
  expect(opacity.middle).toBeLessThan(1);
  await page.keyboard.press('Escape');
  await expect(tooltip).toHaveAttribute('data-state', 'closed');
  await expect(tooltip).toHaveCSS('animation-name', 'ui-tooltip-exit');
  await expect(tooltip).toHaveCount(0);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.mouse.move(0, 0);
  await agent.hover();
  await expect(tooltip).toHaveCSS('animation-name', 'none');
});

test('agent configuration errors are visible without allowing activation', async ({ page }) => {
  await mockManagement(page, { agentConfigError: 'invalid mcp_config.json' });
  await page.goto('/main.html');
  const agent = page.getByRole('button', { name: /DeepSeek Harness/ });
  await expect(agent).toBeVisible();
  await agent.hover();
  await expect(page.getByRole('tooltip')).toContainText('invalid mcp_config.json');
  await expect(page.getByRole('tooltip')).not.toContainText('重试');
  await expect(agent).toHaveAttribute('aria-disabled', 'true');
  const tooltip = page.locator('[data-slot="tooltip-content"]');
  await expect(tooltip).toBeVisible();
  const visibleAtCenter = await tooltip.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) === element;
  });
  expect(visibleAtCenter).toBe(true);
  await page.setViewportSize({ width: 420, height: 800 });
  await agent.hover();
  await expect
    .poll(async () =>
      tooltip.evaluate((element) => {
        const rect = element.getBoundingClientRect();
        return (
          rect.left >= 0 && rect.right <= window.innerWidth && rect.bottom <= window.innerHeight
        );
      }),
    )
    .toBe(true);
  await agent.focus();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await page.mouse.move(0, 0);
  await agent.hover();
  await expect(page.getByRole('tooltip')).toContainText('invalid mcp_config.json');
  await expect(page.locator('.agent-list > .error')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Codex/ }).first()).toBeVisible();
  const before = await page.evaluate(
    () =>
      (window as Window & { managementCommands?: string[] }).managementCommands?.filter(
        (command) => command === 'management_agents_state',
      ).length,
  );
  await agent.dispatchEvent('click');
  await agent.focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Space');
  expect(
    await page.evaluate(
      () =>
        (window as Window & { managementCommands?: string[] }).managementCommands?.filter(
          (command) => command === 'management_agents_state',
        ).length,
    ),
  ).toBe(before);
  expect(
    await page.evaluate(
      () =>
        (window as Window & { managementAgentChanges?: unknown[] }).managementAgentChanges?.length,
    ),
  ).toBe(0);
});

test('a managed agent with invalid configuration can forget its local record', async ({ page }) => {
  await mockManagement(page, {
    agentConfigError: 'invalid mcp_config.json',
    agentConfigManaged: true,
  });
  await page.goto('/main.html');
  const agent = page.getByRole('button', { name: /DeepSeek Harness/ });
  await expect(agent).toHaveClass(/is-on/);
  await agent.hover();
  await expect(page.getByRole('tooltip')).toContainText('只移除 Ossia 的接入记录');
  await agent.click();
  await expect(agent).not.toHaveClass(/is-on/);
  await expect(agent).toHaveAttribute('aria-disabled', 'true');
  await expect(page.locator('.toast-region')).toContainText('宿主配置');
  expect(
    await page.evaluate(
      () => (window as Window & { managementForgottenAgents?: string[] }).managementForgottenAgents,
    ),
  ).toEqual(['dsh']);
});

for (const native of [true, false]) {
  test(`context menus are ${native ? 'disabled in Tauri' : 'available in a browser'}`, async ({
    page,
  }) => {
    if (native) await mockManagement(page);
    await page.goto('/main.html');
    await expect(page.getByRole('button', { name: 'Agent', exact: true })).toBeVisible();

    const results = await page.evaluate(() => {
      const targets = [
        document.body,
        ...['img', 'span', 'input'].map((tag) => {
          const element = document.createElement(tag);
          element.textContent = 'text';
          document.body.appendChild(element);
          return element;
        }),
      ];
      return targets.map((target) => {
        const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
        target.dispatchEvent(event);
        return event.defaultPrevented;
      });
    });
    expect(results).toEqual(Array(4).fill(native));
  });
}

test('browser management cannot invoke native commands', async ({ page }) => {
  await page.goto('/main.html');
  await expect(page.locator('.toast-region')).toContainText('桌面应用');
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
    await expect(page.locator('.toast-region')).toContainText('Open the management window');
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

test('storage settings show usage and run cleanup for the connected backend', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '服务未连接' }).click();
  const dialog = page.getByRole('dialog', { name: '本地服务' });
  await dialog.getByRole('button', { name: '启动服务' }).click();
  await expect(dialog.getByRole('button', { name: '停止服务', exact: true })).toBeEnabled();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await expect(page.getByRole('heading', { name: '存储' })).toBeVisible();
  await expect(
    page.getByText('乐谱库 1.3 GiB · 可复用缓存 95 MiB · 导入与运行数据 0 B'),
  ).toBeVisible();
  await expect(
    page.locator('.settings-row').filter({ hasText: '存储占用' }).locator('.setting-value'),
  ).toHaveCount(0);
  await expect(page.getByText('历史记录：42 条任务记录 · 18 条渲染记录')).toBeVisible();
  await expect(
    page.getByText('上限内的缓存会保留以加速打开；乐谱、导入原件和版本历史始终保留。'),
  ).toBeVisible();
  await expect(
    page.getByText('清理过期任务记录；缓存占用超过设置上限的 75% 后，才开始按最近最少使用淘汰。'),
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
  await expect(page.locator('.toast.is-success')).toContainText(
    '已释放 2.4 MiB；移除 2 条任务和 1 条渲染记录。上限内缓存会保留。',
  );
  await expect(page.locator('.toast')).toHaveCount(1);
  await expect(
    page.getByText('上次清理已完成；上限内缓存继续保留，自动清理按当前设置执行。'),
  ).toBeVisible();
  await expect(page.locator('.settings-page')).not.toContainText(
    '已释放 2.4 MiB；移除 2 条任务和 1 条渲染记录。上限内缓存会保留。',
  );
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

test('storage failures appear as dismissible error toasts', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '服务未连接' }).click();
  const dialog = page.getByRole('dialog', { name: '本地服务' });
  await dialog.getByRole('button', { name: '启动服务' }).click();
  await expect(dialog.getByRole('button', { name: '停止服务', exact: true })).toBeEnabled();
  await page.keyboard.press('Escape');
  await page.evaluate(() => {
    const target = window as unknown as Window & {
      __TAURI_INTERNALS__: {
        invoke: (command: string, args?: unknown) => Promise<unknown>;
      };
    };
    const original = target.__TAURI_INTERNALS__.invoke;
    target.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'management_storage_cleanup') {
        throw { code: 'storage-error', message: 'cache directory is read-only' };
      }
      return original(command, args);
    };
  });
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await page.getByRole('button', { name: '立即清理' }).click();
  const toast = page.locator('.toast.is-error');
  await expect(toast).toContainText('无法清理存储');
  await expect(toast).toContainText('cache directory is read-only');
  await expect(page.locator('.toast')).toHaveCount(1);
  await expect(toast).toHaveCSS('align-items', 'center');
  await toast.getByRole('button', { name: '关闭通知' }).click();
  await expect(toast).toHaveCount(0);
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
    'Agent 配置目录',
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
  await releaseAgentChange(page);
  await expect(page.getByRole('button', { name: '安装 Codex 插件' })).toBeEnabled();
  await page.getByRole('button', { name: '安装 Codex 插件' }).click();
  await releaseAgentChange(page);
  await expect(page.getByRole('button', { name: '移除 Codex 插件' })).toBeEnabled();

  await page.getByRole('button', { name: '安装 DeepSeek Harness 插件' }).click();
  await releaseAgentChange(page);
  await expect(page.locator('.agent-detail-head strong')).toHaveText(['Codex', 'DeepSeek Harness']);
  await page.getByRole('button', { name: '移除 Codex 插件' }).click();
  await releaseAgentChange(page);
  await expect(page.getByRole('button', { name: '安装 Codex 插件' })).toBeEnabled();
  await page.getByRole('button', { name: '安装 Codex 插件' }).click();
  await releaseAgentChange(page);
  await expect(page.locator('.agent-detail-head strong')).toHaveText(['DeepSeek Harness', 'Codex']);
  await expect(page.locator('.agent-detail').first().locator('dd').first()).toHaveText(
    '0.2.2-rc.1 (desktop)',
  );

  await page.getByRole('button', { name: '服务未连接' }).click();
  const dialog = page.getByRole('dialog', { name: '本地服务' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: '关闭', exact: true })).not.toBeFocused();
  await expect(dialog.getByText('LilyPond 工具链')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: '刷新' })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: '运行 doctor' })).toHaveCount(0);
  await dialog.getByRole('button', { name: '打开数据目录' }).click();
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
          expect(control!.x).toBeGreaterThanOrEqual(label!.x + label!.width - 1);
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
