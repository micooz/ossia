import { expect, test } from '@playwright/test';

import { mockManagement } from './management.ts';

test('Agent notices follow the selected language immediately', async ({ page }, testInfo) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '安装 DeepSeek Harness 插件' }).click();
  await expect(page.locator('.agent-detail')).toHaveCount(2);
  await page.getByRole('button', { name: '常规', exact: true }).click();
  await page.getByLabel('语言').selectOption('en-US');
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await expect(page.locator('.agent-detail > .hint')).toHaveText([
    'This integration does not apply to existing Codex sessions. Start a new session or restart Codex.',
    'This integration does not apply to existing DSH sessions. Start a new session or restart DSH.',
  ]);
  await expect(page.locator('.agent-details')).not.toContainText(/[\u3400-\u9fff]/);
  await page.screenshot({ path: testInfo.outputPath('agents-english.png') });
  await page.getByRole('button', { name: 'General', exact: true }).click();
  await page.getByLabel('Language').selectOption('zh-CN');
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await expect(page.locator('.agent-detail > .hint')).toHaveText([
    'Codex 已创建的会话不会生效，请新建会话或重启 Codex。',
    'DSH 已创建的会话不会生效，请新建会话或重启 DSH。',
  ]);
});

test('management empty Agent state', async ({ page }, testInfo) => {
  await mockManagement(page, { empty: true });
  await page.setViewportSize({ width: 1080, height: 720 });
  await page.goto('/main.html');
  await expect(page.locator('.agent-list')).toHaveText(
    '未检测到可接入的 Agent，推荐安装 DeepSeek Harness、Codex',
  );
  await expect(page.locator('.agent-list h2')).toHaveCount(0);
  await expect(page.locator('.agent-chip')).toHaveCount(0);
  await expect(page.locator('.agent-details')).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('agents-empty.png') });
});

test('management initializing Agent detection state', async ({ page }, testInfo) => {
  await mockManagement(page, { empty: true, initializing: true });
  await page.setViewportSize({ width: 1080, height: 720 });
  await page.goto('/main.html');
  await expect(page.locator('.agent-list')).toHaveText('正在检测可接入的 Agent…');
  await expect(page.locator('.agent-list')).toHaveAttribute('aria-busy', 'true');
  await expect(page.locator('.agent-list h2')).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('agents-initializing.png') });
  await page.evaluate(() =>
    (window as Window & { finishAgentDetection?: () => void }).finishAgentDetection?.(),
  );
  await expect(page.locator('.agent-list')).toHaveText(
    '未检测到可接入的 Agent，推荐安装 DeepSeek Harness、Codex',
  );
  await expect(page.locator('.agent-list')).toHaveAttribute('aria-busy', 'false');
});
