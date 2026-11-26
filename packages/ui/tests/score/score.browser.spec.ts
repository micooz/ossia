import { expect, test } from '@playwright/test';

import type { ScorePlayback } from '../../src/generated/protocol.ts';
import {
  firstId,
  hash,
  mockScoreRuntime,
  newer,
  olderVersionId,
  secondId,
} from './score-runtime.ts';

test('score page focuses on one document, all pages and its title', async ({ context, page }) => {
  await mockScoreRuntime(context, { languageEvent: 'en-US' });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.getByRole('button', { name: 'Help' })).toBeVisible();
  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
  await expect(page).toHaveTitle('合成钢琴练习');
  await expect(page.locator('.score-image')).toHaveCount(2);
  await expect(page.locator('.topbar-right .score-actions')).toBeVisible();
  const documentControls = page.locator('.score-document-controls');
  const versionButton = documentControls.getByRole('button', { name: /^Version 2/ });
  await expect(documentControls.locator('.score-title-menu-icon')).toBeVisible();
  await expect(versionButton.locator('.score-version-menu-icon')).toBeVisible();
  await expect(versionButton).toHaveText('Version 2 (current)');
  await expect(
    page.locator('.topbar-right').getByRole('button', { name: 'Export', exact: true }),
  ).toHaveText('');
  await expect(
    page
      .locator('.topbar-right')
      .getByRole('button', { name: 'Export', exact: true })
      .locator('svg'),
  ).toHaveCount(1);
  await expect(page.locator('.score-pages.vertical')).toHaveCount(1);
  await expect(
    page.locator('dialog.shared-dialog').filter({ hasText: /^Help|^帮助/ }),
  ).toBeHidden();
  const titleButton = page.getByRole('button', { name: /Choose a score/ });
  const title = await titleButton.boundingBox();
  const version = await versionButton.boundingBox();
  expect(version!.x).toBeGreaterThan(title!.x + title!.width);
  const selectorStyle = async (locator: typeof titleButton) =>
    locator.evaluate((node) => {
      const style = getComputedStyle(node);
      return {
        color: style.color,
        fontFamily: style.fontFamily,
        fontSize: style.fontSize,
        fontWeight: style.fontWeight,
        paddingInlineStart: style.paddingInlineStart,
      };
    });
  expect(await selectorStyle(versionButton)).toEqual(await selectorStyle(titleButton));
  const header = await page.locator('.score-topbar').boundingBox();
  const featureSlot = await page.locator('.score-feature-slot').boundingBox();
  expect(title!.x).toBeLessThan(featureSlot!.x);
  expect(featureSlot!.x + featureSlot!.width / 2).toBeCloseTo(header!.x + header!.width / 2, 0);
  await versionButton.click();
  await expect(page.getByRole('menuitemradio', { name: /Current draft/ })).toHaveCount(0);
  await expect(page.getByRole('menuitemradio', { name: /Version 2/ })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await expect(page.getByText('Version 2 · current version')).toHaveCount(0);
  await page.emulateMedia({ colorScheme: 'dark' });
  expect(
    await page.evaluate(() =>
      getComputedStyle(globalThis.document.documentElement).getPropertyValue('--page').trim(),
    ),
  ).toBe('#1c1c20');
});

test('copies the score name without opening the score menu', async ({ context, page }) => {
  await mockScoreRuntime(context, { languageEvent: 'en-US' });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.evaluate(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: (text: string) => {
          (window as Window & { copiedText?: string }).copiedText = text;
          return Promise.resolve();
        },
      },
    });
  });
  await page.locator('.score-title-trigger').hover();
  await page.getByRole('button', { name: 'Copy score name' }).click();
  expect(await page.evaluate(() => (window as Window & { copiedText?: string }).copiedText)).toBe(
    '合成钢琴练习',
  );
  await expect(page.getByRole('status').filter({ hasText: 'Score name copied' })).toHaveAttribute(
    'data-success',
    'true',
  );
  await expect(page.getByRole('menu')).toHaveCount(0);
  await page.getByRole('button', { name: /Choose a score/ }).click();
  await expect(page.getByRole('menu')).toBeVisible();
  await page.mouse.move(900, 400);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('menu')).toHaveCount(0);
  await expect(page.locator('.score-title-menu-icon')).toBeVisible();
});

test('restores the score icon after closing the selector', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const trigger = page.locator('.score-title-trigger');
  await trigger.click();
  await expect(page.getByRole('menu')).toBeVisible();
  await page.mouse.move(900, 400);
  await page.keyboard.press('Escape');
  await trigger.focus();
  await expect(page.locator('.score-title-menu-icon')).toBeVisible();
});

test('language settings synchronize across score tabs and survive reload', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context, { language: 'zh-CN' });
  const other = await context.newPage();
  await Promise.all([
    page.goto(`/scores?documentId=${firstId}`),
    other.goto(`/scores?documentId=${secondId}`),
  ]);
  await expect(other.getByRole('button', { name: '乐谱设置' })).toBeVisible();
  await page.getByRole('button', { name: '乐谱设置' }).click();
  const dialog = page.getByRole('dialog', { name: '乐谱设置' });
  await dialog.getByLabel('界面语言').selectOption('en-US');
  await expect(
    page.getByRole('dialog', { name: 'Score settings' }).getByLabel('Interface language'),
  ).toHaveValue('en-US');
  await expect(other.getByRole('button', { name: 'Score settings' })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en-US');
  await other.reload();
  await expect(other.getByRole('button', { name: 'Score settings' })).toBeVisible();
  runtime.rejectLanguage(true);
  await page
    .getByRole('dialog', { name: 'Score settings' })
    .getByLabel('Interface language')
    .selectOption('zh-CN');
  await expect(
    page.getByRole('dialog', { name: 'Score settings' }).getByLabel('Interface language'),
  ).toHaveValue('en-US');
  await expect(page.locator('.toast.is-error')).toBeVisible();
  runtime.rejectLanguage(false);
  await page
    .getByRole('dialog', { name: 'Score settings' })
    .getByLabel('Interface language')
    .selectOption('zh-CN');
  await expect(other.getByRole('button', { name: '乐谱设置' })).toBeVisible();
});

test('score page loads without crypto.randomUUID on a LAN origin', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.addInitScript(() => {
    Object.defineProperty(Crypto.prototype, 'randomUUID', {
      configurable: true,
      value: undefined,
    });
  });
  const pageErrors: string[] = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));

  await page.goto(`/scores?documentId=${firstId}`);

  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
  await expect(page.locator('.score-image')).toHaveCount(2);
  expect(pageErrors).toEqual([]);
});

test('score loading stages use the rendered score background', async ({ context, page }) => {
  await mockScoreRuntime(context);
  let releaseContext!: () => void;
  const contextGate = new Promise<void>((resolve) => {
    releaseContext = resolve;
  });
  await context.route('**/api/v1/documents/*/context', async (route) => {
    await contextGate;
    await route.fallback();
  });
  let releaseVersion!: () => void;
  const versionGate = new Promise<void>((resolve) => {
    releaseVersion = resolve;
  });
  await context.route('**/api/v1/documents/*/versions/*/render', async (route) => {
    await versionGate;
    await route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);
  const connectingHeight = (await page.locator('.score-topbar').boundingBox())!.height;
  const loading = page.locator('.score-loading-state');
  let initialBackground: string;
  try {
    await expect(loading).toBeVisible();
    await expect(loading.locator('.score-loading-note')).toHaveCount(3);
    await expect(loading.getByRole('progressbar')).toHaveCount(1);
    const workspace = await page.locator('.score-workspace').boundingBox();
    const loadingBox = await loading.boundingBox();
    expect(loadingBox!.y + loadingBox!.height / 2).toBeCloseTo(
      workspace!.y + workspace!.height / 2,
      0,
    );
    initialBackground = await page
      .locator('.score-workspace')
      .evaluate((node) => getComputedStyle(node).backgroundColor);
  } finally {
    releaseContext();
  }
  await expect(page.locator('.score-image')).toHaveCount(2);
  const connectedHeight = (await page.locator('.score-topbar').boundingBox())!.height;
  expect(connectedHeight).toBe(connectingHeight);
  const renderedBackground = await page
    .locator('.score-view')
    .evaluate((node) => getComputedStyle(node).backgroundColor);
  expect(initialBackground).toBe(renderedBackground);

  await page.locator('.score-version-control button').click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  try {
    await expect(loading).toBeVisible();
    await expect(page.getByText('载入历史谱面…')).toBeVisible();
    expect(
      await page
        .locator('.score-workspace')
        .evaluate((node) => getComputedStyle(node).backgroundColor),
    ).toBe(renderedBackground);
  } finally {
    releaseVersion();
  }
  await expect(page.locator('.score-image')).toHaveCount(2);
});

test('score header opens the help dialog and returns focus to the score', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const trigger = page.getByRole('button', { name: '帮助', exact: true });
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: '帮助' });
  await expect(dialog).toBeVisible();
  await page.keyboard.press('e');
  await expect(page.getByRole('button', { name: '打开标注' })).toHaveCount(1);
  await expect(dialog.locator('.help-dialog-intro')).toHaveCount(0);
  await expect(dialog.getByRole('heading', { name: '播放' })).toBeVisible();
  await expect(dialog.getByRole('heading', { name: '标注' })).toBeVisible();
  await expect(dialog.getByRole('heading', { name: '视图' })).toBeVisible();
  await expect(dialog.locator('.help-row')).toHaveCount(15);
  await expect(dialog.getByText('选中并试听该音符。')).toBeVisible();
  await expect(dialog.getByText('试听该时刻所有发声的音符，适合听和弦。')).toBeVisible();
  await expect(dialog.getByText('标注单个元素。')).toBeVisible();
  await expect(dialog.getByText('标注多个元素。')).toBeVisible();
  await expect(dialog.getByText('将多处离散元素添加到同一分组标注中。')).toBeVisible();
  await expect(dialog.getByText('标注单个元素。')).toHaveCSS('text-align', 'left');
  const descriptionLefts = await dialog
    .locator('.help-row dd')
    .evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().left));
  expect(Math.max(...descriptionLefts) - Math.min(...descriptionLefts)).toBeLessThan(0.5);
  const keys = await dialog.locator('.help-trigger').allTextContents();
  expect(keys.slice(0, 4)).toEqual(['点击音符', '← / →', 'Alt + 点击音符', 'Space']);
  expect(keys[4]).toBe('E');
  expect(keys[5]).toBe('点击元素');
  expect(keys[6]).toBe('框选');
  expect(keys[7]).toMatch(/^(Command|Ctrl) \+ 点击\/框选$/);
  expect(keys[8]).toBe('Esc');
  expect(keys[9]).toMatch(/^(Command|Ctrl) \+ 拖拽$/);
  expect(keys[10]).toMatch(/^(Command|Ctrl) \+ 滚轮$/);
  expect(keys.slice(11, 13)).toEqual(['D', 'R']);
  expect(keys[13]).toMatch(/^(Command|Ctrl) \+ 绘画$/);
  expect(keys[14]).toBe('Esc');

  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('main')).toBeFocused();

  await trigger.click();
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('main')).toBeFocused();
});

test('open dialogs block page gestures and scroll dialog content without page movement', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 900, height: 520 });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '帮助', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '帮助' });
  await expect(dialog).toBeVisible();

  const dispatch = (type: string, touchCount: number) =>
    page.evaluate(
      ({ type, touchCount }) => {
        const event = new Event(type, { bubbles: true, cancelable: true });
        const touches = Array.from({ length: touchCount }, () => ({}));
        Object.defineProperties(event, {
          touches: { value: touches },
          changedTouches: { value: touches },
        });
        document.dispatchEvent(event);
        return event.defaultPrevented;
      },
      { type, touchCount },
    );

  const dispatchTouch = (target: 'backdrop' | 'content', type: string) =>
    page.evaluate(
      ({ target, type }) => {
        const dialogElement = document.querySelector<HTMLDialogElement>('dialog[open]')!;
        const targetElement =
          target === 'content'
            ? dialogElement.querySelector<HTMLElement>('.shared-dialog-body')!
            : dialogElement;
        const bounds = targetElement.getBoundingClientRect();
        const touch = {
          clientX: target === 'backdrop' ? 1 : bounds.left + bounds.width / 2,
          clientY:
            target === 'backdrop'
              ? 1
              : type === 'touchstart'
                ? bounds.bottom - 20
                : bounds.top + 20,
        };
        const event = new Event(type, { bubbles: true, cancelable: true });
        Object.defineProperties(event, {
          touches: { value: type === 'touchend' ? [] : [touch] },
          changedTouches: { value: [touch] },
        });
        targetElement.dispatchEvent(event);
        return event.defaultPrevented;
      },
      { target, type },
    );

  expect(await dispatch('gesturestart', 2)).toBe(true);
  expect(await dispatch('gesturechange', 2)).toBe(true);
  expect(await dispatch('gestureend', 2)).toBe(true);
  expect(await dispatch('touchstart', 2)).toBe(true);
  expect(await dispatch('touchmove', 2)).toBe(true);
  expect(await dispatch('touchend', 2)).toBe(true);
  expect(await dispatch('touchmove', 1)).toBe(true);
  expect(await dispatchTouch('backdrop', 'touchstart')).toBe(true);
  expect(await dispatchTouch('backdrop', 'touchmove')).toBe(true);
  expect(await dispatchTouch('backdrop', 'touchend')).toBe(true);
  expect(await dispatchTouch('content', 'touchstart')).toBe(false);
  expect(await dispatchTouch('content', 'touchmove')).toBe(true);
  expect(
    await dialog.locator('.shared-dialog-body').evaluate((node) => node.scrollTop),
  ).toBeGreaterThan(0);
  expect(await page.evaluate(() => window.scrollY)).toBe(0);
  expect(await dispatchTouch('content', 'touchend')).toBe(false);

  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toBeHidden();
  expect(await dispatch('gesturestart', 2)).toBe(false);
});

test('QR code button is disabled until LAN access is enabled', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const qr = page.getByRole('button', { name: '请先在桌面设置中开启局域网访问' });
  const help = page.getByRole('button', { name: '帮助', exact: true });
  await expect(qr).toBeDisabled();
  await expect(qr).toHaveAttribute('data-tooltip', '请先在桌面设置中开启局域网访问');
  const qrBox = await qr.boundingBox();
  const helpBox = await help.boundingBox();
  expect(qrBox!.x + qrBox!.width).toBeLessThan(helpBox!.x);
});

test('right header icon buttons expose custom tooltips', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const buttons = [
    { selector: '.topbar-export', tooltip: '导出' },
    { selector: '.topbar-qr', tooltip: '请先在桌面设置中开启局域网访问' },
    { selector: '.topbar-help', tooltip: '帮助' },
  ];
  expect(
    await page.locator('.score-topbar').evaluate((node) => Number(getComputedStyle(node).zIndex)),
  ).toBeGreaterThan(0);
  for (const { selector, tooltip } of buttons) {
    const button = page.locator(selector);
    await expect(button).toHaveAttribute('data-tooltip', tooltip);
    await button.hover();
    await expect
      .poll(() => button.evaluate((node) => getComputedStyle(node, '::after').opacity))
      .toBe('1');
    await expect
      .poll(() => button.evaluate((node) => getComputedStyle(node, '::after').content))
      .toBe(`"${tooltip}"`);
  }
});

test('score toolbar buttons do not draw focus outlines', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const annotation = page.getByRole('button', { name: '打开标注', exact: true });
  for (let index = 0; index < 8; index++) {
    if (await annotation.evaluate((node) => node === document.activeElement)) break;
    await page.keyboard.press('Tab');
  }
  await expect(annotation).toBeFocused();
  expect(
    await annotation.evaluate((node) => ({
      focusVisible: node.matches(':focus-visible'),
      outlineStyle: getComputedStyle(node).outlineStyle,
    })),
  ).toEqual({ focusVisible: true, outlineStyle: 'none' });
});

test('touch-only input hides tooltips and shows touch gestures in help', async ({ browser }) => {
  const context = await browser.newContext({
    baseURL: 'http://127.0.0.1:5173',
    hasTouch: true,
    isMobile: true,
    locale: 'zh-CN',
    viewport: { width: 834, height: 1194 },
  });
  const page = await context.newPage();
  await mockScoreRuntime(context);
  try {
    await page.goto(`/scores?documentId=${firstId}`);
    expect(
      await page.evaluate(() => matchMedia('(hover: none) and (pointer: coarse)').matches),
    ).toBe(true);

    const exportButton = page.locator('.topbar-export');
    await expect(exportButton).toHaveAttribute('data-tooltip', '导出');
    expect(await exportButton.evaluate((node) => getComputedStyle(node, '::after').display)).toBe(
      'none',
    );
    const header = await page.locator('.score-topbar').evaluate((node) => {
      const style = getComputedStyle(node);
      return { touchAction: style.touchAction, userSelect: style.userSelect };
    });
    expect(header).toEqual({ touchAction: 'none', userSelect: 'none' });

    const pen = page.getByRole('button', { name: '画笔', exact: true });
    await expect(pen).toBeVisible();
    const penBounds = (await pen.boundingBox())!;
    expect(penBounds.width).toBeGreaterThanOrEqual(44);
    expect(penBounds.height).toBeGreaterThanOrEqual(44);

    await page.getByRole('button', { name: '帮助', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '帮助' });
    await expect(dialog.locator('.help-row')).toHaveCount(6);
    expect(await dialog.locator('.help-trigger').allTextContents()).toEqual([
      '点击音符',
      '点击元素',
      '框选',
      '单指拖动',
      '双指捏合',
      '手指或触控笔',
    ]);
    await expect(dialog.getByRole('heading', { name: '视图' })).toBeVisible();
    await expect(dialog.getByText('← / →')).toHaveCount(0);
    await expect(dialog.getByText('适合鼠标场景。')).toHaveCount(0);
  } finally {
    await context.close();
  }
});

test('QR panel opens the current score LAN link and remains scannable in dark mode', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context, { webBaseUrl: 'http://192.168.1.20:3211' });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto(`/scores?documentId=${firstId}`);
  const trigger = page.getByRole('button', { name: '二维码', exact: true });
  await expect(trigger).toBeEnabled();
  await trigger.click();
  const panel = page.getByRole('dialog', { name: '二维码' });
  await expect(panel).toBeVisible();
  await expect(panel.getByText('扫码在手机上打开')).toHaveCount(0);
  const code = panel.locator('.score-qr-panel-code');
  await expect(code).toHaveAttribute(
    'data-qr-url',
    `http://192.168.1.20:3211/scores?documentId=${firstId}`,
  );
  await expect(code.locator('svg')).toBeVisible();
  await expect(code).toHaveCSS('background-color', 'rgb(255, 255, 255)');
  await expect(panel.getByText('请确保设备与这台电脑连接同一局域网。')).toBeVisible();
  const box = await panel.boundingBox();
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(390);

  await page.keyboard.press('Escape');
  await expect(panel).toBeHidden();
  await expect(page.getByRole('main')).toBeFocused();
});

test('score title menu refreshes every time and keeps cached scores when refresh fails', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  let documentRequests = 0;
  let failRefresh = false;
  await context.route('**/api/v1/documents*', (route) => {
    documentRequests += 1;
    return failRefresh ? route.abort('failed') : route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);

  const title = page.getByRole('button', { name: /选择乐谱/ });
  await title.click();
  await expect(page.getByRole('menuitemradio', { name: '合成钢琴练习' })).toBeVisible();
  expect(documentRequests).toBe(1);

  await page.keyboard.press('Escape');
  failRefresh = true;
  await title.click();
  await expect.poll(() => documentRequests).toBe(2);
  await page.waitForTimeout(100);

  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('menuitem', { name: '重试', exact: true })).toHaveCount(0);
  await expect(page.getByRole('menuitemradio', { name: '合成钢琴练习' })).toBeVisible();
});

test('score title menu sorts documents by modified time descending', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);

  await page.getByRole('button', { name: /选择乐谱/ }).click();
  await expect(page.getByRole('menuitemradio')).toHaveText(['另一份乐谱', '合成钢琴练习']);
});

test('score title menu selects a stationary pen tap without a browser click', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: /选择乐谱/ }).click();
  const item = page.getByRole('menuitemradio', { name: '另一份乐谱' });
  await expect(item).toBeVisible();
  const box = (await item.boundingBox())!;
  const pen = { pointerType: 'pen', pointerId: 7, clientX: box.x + 10, clientY: box.y + 10 };
  await item.dispatchEvent('pointerdown', { ...pen, button: 0, buttons: 1 });
  await item.dispatchEvent('pointermove', { ...pen, clientY: pen.clientY + 20, buttons: 1 });
  await item.dispatchEvent('pointerup', { ...pen, button: 0, buttons: 0 });
  await expect(page).toHaveURL(new RegExp(`documentId=${firstId}`));
  await item.dispatchEvent('pointerdown', { ...pen, button: 0, buttons: 1 });
  await item.dispatchEvent('pointercancel', pen);
  await item.dispatchEvent('pointerup', { ...pen, button: 0, buttons: 0 });
  await expect(page).toHaveURL(new RegExp(`documentId=${firstId}`));
  await item.dispatchEvent('pointerdown', { ...pen, button: 0, buttons: 1 });
  await item.dispatchEvent('pointerup', { ...pen, button: 0, buttons: 0 });
  await expect(page).toHaveURL(new RegExp(`documentId=${secondId}`));
});

test('score title menu switches documents and follows browser history', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const title = page.getByRole('button', { name: /选择乐谱/ });
  await title.click();
  await expect(page.getByRole('menuitemradio', { name: '合成钢琴练习' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await page.getByRole('menuitemradio', { name: '另一份乐谱' }).click();
  await expect(page.locator('.topbar-title')).toHaveText('另一份乐谱');
  await expect(page).toHaveURL(new RegExp(`documentId=${secondId}`));
  await expect(title).not.toBeFocused();
  expect(await title.evaluate((node) => getComputedStyle(node).outlineStyle)).toBe('none');
  await page.goBack();
  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
  await expect(page).toHaveURL(new RegExp(`documentId=${firstId}`));
  await page.goForward();
  await expect(page.locator('.topbar-title')).toHaveText('另一份乐谱');
  await expect(page).toHaveURL(new RegExp(`documentId=${secondId}`));
});

test('missing and unknown documents use the visual 404 page and the old route is gone', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto('/scores');
  await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  await expect(page).toHaveTitle('页面不存在 · Ossia');
  await expect(page.getByRole('button', { name: '帮助' })).toBeVisible();
  await page.goto('/scores?documentId=55555555-5555-4555-8555-555555555555');
  await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  await expect(page.getByRole('button', { name: '帮助' })).toBeVisible();
  const legacy = await page.request.get(`/score.html?documentId=${firstId}`);
  expect(legacy.status()).toBe(404);
});

test('deleted and restored scores follow document lifecycle events', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);

  runtime.trash();
  await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();

  runtime.untrash();
  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
  await expect(page.locator('.score-image')).toHaveCount(2);
});

test('double click on the rendered score does not select the page', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const host = page.locator('.page-host').first();
  const box = await host.boundingBox();
  expect(box).not.toBeNull();
  await expect(page.locator('.score-scroll')).toHaveCSS('user-select', 'none');
  await host.dblclick({ position: { x: box!.width / 2, y: box!.height / 2 } });
  expect(
    await page.evaluate(() => {
      const selection = window.getSelection();
      return !!selection && selection.rangeCount > 0 && !selection.isCollapsed;
    }),
  ).toBe(false);
});

test('touch drag pans the score without native page gestures', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 900, height: 800 });
  await page.goto(`/scores?documentId=${firstId}`);
  const scroll = page.locator('.score-scroll');
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  await expect(scroll).toHaveCSS('touch-action', 'none');
  const before = (await host.boundingBox())!;

  await scroll.evaluate((node) => {
    const fire = (type: string, clientX: number, clientY: number) =>
      node.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          pointerId: 1,
          pointerType: 'touch',
          isPrimary: true,
          button: 0,
          buttons: type === 'pointerup' ? 0 : 1,
          clientX,
          clientY,
        }),
      );
    fire('pointerdown', 500, 400);
    fire('pointermove', 580, 350);
    fire('pointerup', 580, 350);
  });

  await expect.poll(async () => (await host.boundingBox())!.x).toBeGreaterThan(before.x + 70);
  await expect.poll(async () => (await host.boundingBox())!.y).toBeLessThan(before.y - 40);
});

test('two-finger touch pinch zooms around the gesture midpoint', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  const scroll = page.locator('.score-scroll');
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  const before = (await host.boundingBox())!;
  const center = { x: before.x + before.width / 2, y: before.y + before.height / 2 };

  await scroll.evaluate((node, center) => {
    const fire = (type: string, pointerId: number, clientX: number, clientY: number) =>
      node.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          pointerId,
          pointerType: 'touch',
          isPrimary: pointerId === 1,
          button: 0,
          buttons: type === 'pointerup' ? 0 : 1,
          clientX,
          clientY,
        }),
      );
    fire('pointerdown', 1, center.x - 60, center.y);
    fire('pointerdown', 2, center.x + 60, center.y);
    fire('pointermove', 1, center.x - 120, center.y);
    fire('pointermove', 2, center.x + 120, center.y);
    fire('pointerup', 1, center.x - 120, center.y);
    fire('pointerup', 2, center.x + 120, center.y);
  }, center);

  await expect(page.getByLabel('谱面缩放')).toHaveText('200%');
  await expect
    .poll(async () => (await host.boundingBox())!.width)
    .toBeGreaterThan(before.width * 1.9);
  const after = (await host.boundingBox())!;
  expect(after.x + after.width / 2).toBeCloseTo(center.x, 0);
  expect(after.y + after.height / 2).toBeCloseTo(center.y, 0);
});

test('Safari gesture events zoom the score', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  const scroll = page.locator('.score-scroll');
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  const before = (await host.boundingBox())!;
  const center = { x: before.x + before.width / 2, y: before.y + before.height / 2 };

  await scroll.evaluate((node, center) => {
    const fire = (type: string, scale: number) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperties(event, {
        scale: { value: scale },
        clientX: { value: center.x },
        clientY: { value: center.y },
      });
      node.dispatchEvent(event);
    };
    fire('gesturestart', 1);
    fire('gesturechange', 1.5);
    fire('gestureend', 1.5);
  }, center);

  await expect(page.getByLabel('谱面缩放')).toHaveText('150%');
  const after = (await host.boundingBox())!;
  expect(after.x + after.width / 2).toBeCloseTo(center.x, 0);
  expect(after.y + after.height / 2).toBeCloseTo(center.y, 0);
});

test('layout, zoom and modifier pan apply to the whole document', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 900, height: 800 });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const vertical = await page
    .locator('.page-host')
    .evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().toJSON()));
  expect(vertical[0]!.x).toBeCloseTo(vertical[1]!.x, 0);
  expect(vertical[1]!.y).toBeGreaterThan(vertical[0]!.y + vertical[0]!.height);
  expect(vertical[1]!.y - (vertical[0]!.y + vertical[0]!.height)).toBeCloseTo(24, 0);
  await expect(page.locator('.score-toolbar')).toHaveCount(0);
  await page.locator('.score-view').hover();
  await page.getByRole('button', { name: '横向' }).click();
  await expect(page.locator('.score-pages.horizontal')).toHaveCount(1);
  const horizontal = await page
    .locator('.page-host')
    .evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().toJSON()));
  expect(horizontal[0]!.y).toBeCloseTo(horizontal[1]!.y, 0);
  expect(horizontal[1]!.x).toBeGreaterThan(horizontal[0]!.x + horizontal[0]!.width);
  expect(horizontal[1]!.x - (horizontal[0]!.x + horizontal[0]!.width)).toBeCloseTo(24, 0);
  const firstSize = { width: horizontal[0]!.width, height: horizontal[0]!.height };
  const scroll = page.locator('.score-scroll');
  await scroll.hover();
  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -100);
  await page.keyboard.up('Control');
  await expect(page.getByLabel('谱面缩放')).toHaveText('120%');
  await expect
    .poll(async () => (await page.locator('.page-host').first().boundingBox())!.height)
    .toBeGreaterThan(firstSize.height * 1.15);
  await scroll.dispatchEvent('wheel', {
    deltaY: -1,
    ctrlKey: true,
    clientX: 450,
    clientY: 400,
  });
  await expect(page.getByLabel('谱面缩放')).toHaveText('121%');
  await page.keyboard.down('Control');
  for (let index = 0; index < 11; index++) {
    await page.mouse.wheel(0, -100);
    await page.waitForTimeout(10);
  }
  await expect(page.getByLabel('谱面缩放')).toHaveText('400%');
  await scroll.dispatchEvent('wheel', {
    deltaY: 1,
    ctrlKey: true,
    clientX: 450,
    clientY: 400,
  });
  await expect(page.getByLabel('谱面缩放')).toHaveText('396%');
  for (let index = 0; index < 16; index++) {
    await page.mouse.wheel(0, 100);
    await page.waitForTimeout(10);
  }
  await page.keyboard.up('Control');
  await expect(page.getByLabel('谱面缩放')).toHaveText('25%');
  await page.keyboard.down('Control');
  for (let index = 0; index < 10; index++) {
    await page.mouse.wheel(0, -100);
    await page.waitForTimeout(10);
  }
  await page.keyboard.up('Control');
  const beforePan = (await page.locator('.page-host').first().boundingBox())!.x;
  await page.keyboard.down('Control');
  await page.mouse.move(700, 400);
  await page.mouse.down();
  await page.mouse.move(500, 350);
  await page.mouse.up();
  await page.keyboard.up('Control');
  await expect
    .poll(async () => (await page.locator('.page-host').first().boundingBox())!.x)
    .toBeLessThan(beforePan - 150);
});

test('modifier wheel zoom keeps the score under the pointer', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  const scroll = page.locator('.score-scroll');
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  const pointer = await host.evaluate((node) => {
    const rect = node.getBoundingClientRect();
    return {
      x: rect.left + rect.width * 0.62,
      y: rect.top + rect.height * 0.32,
      fractionX: 0.62,
      fractionY: 0.32,
    };
  });
  for (let index = 0; index < 2; index++) {
    await scroll.dispatchEvent('wheel', {
      deltaY: -1,
      ctrlKey: true,
      clientX: pointer.x,
      clientY: pointer.y,
    });
    await page.waitForTimeout(10);
  }
  await expect(page.getByLabel('谱面缩放')).toHaveText('102%');
  const drift = await host.evaluate((node, anchor) => {
    const rect = node.getBoundingClientRect();
    return {
      x: rect.left + rect.width * anchor.fractionX - anchor.x,
      y: rect.top + rect.height * anchor.fractionY - anchor.y,
    };
  }, pointer);
  expect(Math.abs(drift.x)).toBeLessThan(4);
  expect(Math.abs(drift.y)).toBeLessThan(4);
});

test('trackpad zoom changes the score smoothly', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  const scroll = page.locator('.score-scroll');
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  const widths = [(await host.boundingBox())!.width];
  for (let index = 0; index < 5; index++) {
    await scroll.dispatchEvent('wheel', {
      deltaY: -1,
      ctrlKey: true,
      clientX: 650,
      clientY: 400,
    });
    await page.waitForTimeout(20);
    widths.push((await host.boundingBox())!.width);
  }
  for (let index = 1; index < widths.length; index++) {
    expect(widths[index]!).toBeGreaterThan(widths[index - 1]!);
    expect(widths[index]! - widths[index - 1]!).toBeLessThan(20);
  }
});

test('resizing preserves the current score position', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  const scroll = page.locator('.score-scroll');
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  for (let index = 0; index < 3; index++) {
    await scroll.dispatchEvent('wheel', {
      deltaY: -100,
      ctrlKey: true,
      clientX: 650,
      clientY: 450,
    });
    await page.waitForTimeout(10);
  }
  await expect(page.getByLabel('谱面缩放')).toHaveText('173%');
  await scroll.dispatchEvent('wheel', { deltaX: 180, deltaY: 140 });
  await page.waitForTimeout(50);
  const before = (await host.boundingBox())!;
  const transform = await page.locator('.score-pages').evaluate((node) => node.style.transform);
  expect(transform).not.toBe('translate(0px, 0px)');

  await page.setViewportSize({ width: 1000, height: 800 });
  await expect
    .poll(() => page.locator('.score-pages').evaluate((node) => node.style.transform))
    .toBe(transform);
  await expect.poll(async () => (await host.boundingBox())!.x).toBeCloseTo(before.x, 0);
  await expect.poll(async () => (await host.boundingBox())!.y).toBeCloseTo(before.y, 0);
});

test('zoomed score can be panned without scrollbar bounds', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  const scroll = page.locator('.score-scroll');
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  const bounds = await host.boundingBox();
  for (let index = 0; index < 2; index++) {
    await scroll.dispatchEvent('wheel', {
      deltaY: -100,
      ctrlKey: true,
      clientX: bounds!.x + 40,
      clientY: bounds!.y + 200,
    });
    await page.waitForTimeout(10);
  }
  const before = (await host.boundingBox())!.x;
  await page.keyboard.down('Control');
  await page.mouse.move(800, 400);
  await page.mouse.down();
  await page.mouse.move(400, 400);
  await page.mouse.up();
  await page.keyboard.up('Control');
  await expect.poll(async () => (await host.boundingBox())!.x).toBeLessThan(before - 350);
  expect(await scroll.evaluate((node) => getComputedStyle(node).overflow)).toBe('hidden');
});

test('modifier drag pans the score when it fits the viewport', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  const host = page.locator('.page-host').first();
  await expect(host).toBeVisible();
  const before = (await host.boundingBox())!.x;
  await page.keyboard.down('Control');
  await page.mouse.move(700, 400);
  await page.mouse.down();
  await page.mouse.move(620, 400);
  await page.mouse.up();
  await page.keyboard.up('Control');
  await expect.poll(async () => (await host.boundingBox())!.x).toBeLessThan(before - 60);
});

test('narrow topbar keeps one row and collapses the score selector to a menu icon', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 640, height: 800 });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.topbar-brand, .topbar-brand-label')).toHaveCount(0);
  const triggerElement = page.locator('.score-title-trigger');
  await expect(triggerElement.locator('.score-title-label')).toBeHidden();
  await expect(triggerElement.locator('.score-title-menu-icon')).toBeVisible();
  const trigger = await triggerElement.boundingBox();
  const toolbar = await page.locator('.score-feature-toolbar').boundingBox();
  const right = await page.locator('.topbar-right').boundingBox();
  expect(trigger!.x + trigger!.width).toBeLessThan(right!.x);
  expect(toolbar!.y).toBeLessThan(trigger!.y + trigger!.height);
  expect(trigger!.y).toBeLessThan(toolbar!.y + toolbar!.height);
  expect(right!.y).toBeLessThan(toolbar!.y + toolbar!.height);
  expect(toolbar!.x + toolbar!.width / 2).toBeCloseTo(
    await page.evaluate(() => window.innerWidth / 2),
    0,
  );
});

test('fullscreen remains available for draft and historical scores with a focus-restoring exit', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.evaluate(() => {
    Object.defineProperty(document.documentElement, 'requestFullscreen', {
      configurable: true,
      value: undefined,
    });
  });
  const fullscreen = page.getByRole('button', { name: '全屏', exact: true });
  await expect(fullscreen).toBeVisible();
  await expect(
    page.locator('.score-feature-toolbar').getByRole('button', { name: '全屏' }),
  ).toHaveCount(1);
  await expect(page.locator('.topbar-right').getByRole('button', { name: '全屏' })).toHaveCount(0);
  await fullscreen.click();
  await expect(page.locator('.score-topbar')).toBeHidden();
  await expect(page.locator('.score-feature-toolbar')).toHaveClass(/is-floating/);
  const floatingToolbar = await page.locator('.score-feature-toolbar').boundingBox();
  expect(floatingToolbar!.y).toBeGreaterThanOrEqual(30);
  const exit = page
    .locator('.score-feature-toolbar')
    .getByRole('button', { name: '退出全屏', exact: true });
  await expect(exit).toBeVisible();
  const app = await page.locator('.score-app').boundingBox();
  const viewport = page.viewportSize()!;
  expect(app!.height).toBeCloseTo(viewport.height, 0);
  await exit.click();
  await expect(page.locator('.score-topbar')).toBeVisible();
  await expect(fullscreen).toBeFocused();

  await page.locator('.score-version-control button').click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await expect(page.getByRole('button', { name: '恢复至此版本' })).toBeVisible();
  await expect(fullscreen).toBeVisible();
  await fullscreen.click();
  await expect(page.locator('.score-topbar')).toBeHidden();
  await expect(page.getByRole('button', { name: '退出全屏', exact: true })).toBeVisible();
});

for (const device of [
  {
    name: 'iPad',
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)',
    platform: 'MacIntel',
  },
  { name: 'iPhone', userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)', platform: 'iPhone' },
  {
    name: 'Android tablet',
    userAgent: 'Mozilla/5.0 (Linux; Android 14; Tablet)',
    platform: 'Linux',
  },
  {
    name: 'Android phone',
    userAgent: 'Mozilla/5.0 (Linux; Android 14; Mobile)',
    platform: 'Linux',
  },
]) {
  test(`${device.name} uses immersive mode without requesting native fullscreen`, async ({
    context,
    page,
  }) => {
    await mockScoreRuntime(context);
    await page.addInitScript(({ userAgent, platform }) => {
      Object.defineProperty(navigator, 'userAgent', { configurable: true, value: userAgent });
      Object.defineProperty(navigator, 'platform', { configurable: true, value: platform });
      Object.defineProperty(navigator, 'maxTouchPoints', { configurable: true, value: 5 });
      Object.defineProperty(Document.prototype, 'fullscreenEnabled', {
        configurable: true,
        get: () => true,
      });
      Object.defineProperty(HTMLElement.prototype, 'requestFullscreen', {
        configurable: true,
        value: () => {
          document.documentElement.dataset.fullscreenRequested = 'true';
          return Promise.resolve();
        },
      });
    }, device);
    await page.goto(`/scores?documentId=${firstId}`);
    await page.getByRole('button', { name: '全屏', exact: true }).click();
    await expect(page.locator('.score-topbar')).toBeHidden();
    await expect(page.locator('html')).not.toHaveAttribute('data-fullscreen-requested');
    await expect(page.getByRole('button', { name: '退出全屏', exact: true })).toBeVisible();
    await page.getByRole('button', { name: '退出全屏', exact: true }).click();
    await expect(page.locator('.score-topbar')).toBeVisible();
  });
}

test('touchscreen PC uses native fullscreen and follows external exit', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
    });
    Object.defineProperty(navigator, 'platform', { configurable: true, value: 'Win32' });
    Object.defineProperty(navigator, 'maxTouchPoints', { configurable: true, value: 5 });
    let fullscreenElement: Element | null = null;
    Object.defineProperty(Document.prototype, 'fullscreenEnabled', {
      configurable: true,
      get: () => true,
    });
    Object.defineProperty(Document.prototype, 'fullscreenElement', {
      configurable: true,
      get: () => fullscreenElement,
    });
    Object.defineProperty(HTMLElement.prototype, 'requestFullscreen', {
      configurable: true,
      value() {
        fullscreenElement = document.documentElement;
        document.dispatchEvent(new Event('fullscreenchange'));
        return Promise.resolve();
      },
    });
    Object.defineProperty(Document.prototype, 'exitFullscreen', {
      configurable: true,
      value() {
        fullscreenElement = null;
        document.dispatchEvent(new Event('fullscreenchange'));
        return Promise.resolve();
      },
    });
    (window as Window & { exitNativeFullscreen?: () => void }).exitNativeFullscreen = () => {
      fullscreenElement = null;
      document.dispatchEvent(new Event('fullscreenchange'));
    };
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '全屏', exact: true }).click();
  await expect(page.locator('.score-topbar')).toBeHidden();
  await page.evaluate(() =>
    (window as Window & { exitNativeFullscreen?: () => void }).exitNativeFullscreen?.(),
  );
  await expect(page.locator('.score-topbar')).toBeVisible();
  await expect(page.getByRole('button', { name: '全屏', exact: true })).toBeVisible();
});

test('floating controls change zoom with buttons and a menu', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const score = page.locator('.score-view');
  await score.hover();
  await expect(page.locator('.score-controls')).toHaveCSS('opacity', '1');
  const zoom = page.getByLabel('谱面缩放');
  await expect(zoom).toHaveText('100%');
  await expect(zoom.locator('svg')).toHaveCount(0);
  await page.getByRole('button', { name: '放大' }).click();
  await expect(zoom).toHaveText('110%');
  await page.getByRole('button', { name: '缩小' }).click();
  await expect(zoom).toHaveText('100%');
  await zoom.click();
  await expect(page.locator('.score-controls')).toHaveCSS('opacity', '1');
  await page.getByRole('menuitemradio', { name: '200%' }).click();
  await expect(zoom).toHaveText('200%');
});

test('version selection, restore order and export dialog use the viewed version', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const versionButton = page.locator('.score-version-control').getByRole('button');
  const versionControl = page.locator('.score-version-control');
  await expect(versionButton).toHaveText('版本 2（当前）');
  await expect(versionButton).toHaveAccessibleName('版本 2（当前）');
  const normalColor = await versionButton.evaluate((node) => getComputedStyle(node).color);
  const brandColor = await page.evaluate(() => {
    const probe = document.createElement('span');
    probe.style.color = 'color-mix(in srgb, var(--accent) 82%, var(--ink))';
    document.body.append(probe);
    const color = getComputedStyle(probe).color;
    probe.remove();
    return color;
  });
  await versionButton.click();
  await expect(page.getByRole('menuitemradio', { name: /当前草稿/ })).toHaveCount(0);
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  const restore = page.getByRole('button', { name: '恢复至此版本' });
  await expect(restore).toBeVisible();
  await expect(versionButton).not.toBeFocused();
  expect(await versionButton.evaluate((node) => getComputedStyle(node).outlineStyle)).toBe('none');
  await expect(versionButton).toHaveText('版本 1');
  await expect(versionControl).toHaveAttribute('data-viewing-version', 'true');
  expect(await versionControl.evaluate((node) => getComputedStyle(node).backgroundColor)).not.toBe(
    'rgba(0, 0, 0, 0)',
  );
  expect(await versionButton.evaluate((node) => getComputedStyle(node).color)).toBe(brandColor);
  expect(
    await versionButton
      .locator('.score-selector-chevron')
      .evaluate((node) => getComputedStyle(node).color),
  ).toBe(brandColor);
  expect(
    await versionButton
      .locator('.score-selector-label')
      .evaluate((node) => getComputedStyle(node).color),
  ).toBe(brandColor);
  await expect(page.locator('.score-context-region')).toHaveCount(0);
  await expect(
    page.locator('.topbar-right').getByRole('button', { name: '恢复至此版本' }),
  ).toHaveCount(0);
  await versionButton.click();
  await page.getByRole('menuitemradio', { name: /版本 2/ }).click();
  await expect(restore).toHaveCount(0);
  await expect(versionButton).toHaveText('版本 2（当前）');
  await expect(versionControl).not.toHaveAttribute('data-viewing-version');
  await expect(versionButton).toHaveCSS('color', normalColor);
  await versionButton.click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await page.getByRole('button', { name: '导出' }).click();
  const dialog = page.locator('.export-dialog');
  await expect(dialog.getByRole('heading', { name: '导出', exact: true })).toBeVisible();
  await expect(dialog.getByRole('heading', { name: '导出', exact: true })).toBeFocused();
  expect(
    await dialog
      .getByRole('button', { name: '关闭' })
      .evaluate((node) => getComputedStyle(node).outlineStyle),
  ).toBe('none');
  await expect(dialog.getByRole('navigation', { name: '导出格式' })).toBeVisible();
  await expect(dialog.getByRole('heading', { name: '导出设置' })).toHaveCount(0);
  await expect(dialog.getByRole('heading', { name: '导出预览' })).toHaveCount(0);
  await expect(dialog.locator('.export-dialog-header p')).toHaveCount(0);
  expect(await dialog.locator('.export-format strong').allTextContents()).toEqual([
    'PDF',
    'PNG',
    'SVG',
    'MIDI',
    'WAV',
  ]);
  await expect(dialog.locator('.export-preview-stage img')).toHaveCount(2);
  await expect(dialog.getByText('第 1 页 / 共 2 页')).toBeVisible();
  await expect(dialog.locator('.export-page-count')).toHaveCSS('border-top-width', '0px');
  await dialog.locator('.export-preview-stage').evaluate((stage) => {
    stage.scrollTop = stage.scrollHeight;
  });
  await expect(dialog.getByText('第 2 页 / 共 2 页')).toBeVisible();
  const includeDrawing = dialog.getByRole('checkbox', { name: '包含绘画' });
  await expect(includeDrawing).toBeChecked();
  await expect
    .poll(() =>
      dialog.evaluate((node) => {
        const drawingRow = node.querySelector('label[for="export-include-drawing"]')!;
        const passwordLabel = node.querySelector('label[for="export-pdf-password"]')!;
        return (
          drawingRow.getBoundingClientRect().top - passwordLabel.getBoundingClientRect().bottom
        );
      }),
    )
    .toBeGreaterThanOrEqual(10);
  await includeDrawing.focus();
  await page.keyboard.press('Space');
  await expect(includeDrawing).not.toBeChecked();
  await page.keyboard.press('Space');
  await expect(includeDrawing).toBeChecked();
  const password = dialog.getByLabel('PDF 密码');
  expect(await password.evaluate((input: HTMLInputElement) => input.form !== null)).toBe(true);
  await password.fill('secret');
  await dialog.getByRole('button', { name: '显示密码' }).click();
  await expect(password).toHaveAttribute('type', 'text');
  await expect(password).toHaveValue('secret');
  await dialog.getByRole('button', { name: '隐藏密码' }).click();
  await expect(password).toHaveAttribute('type', 'password');
  await dialog.getByRole('checkbox', { name: '导出指定页面' }).check();
  await dialog.getByLabel('起始页').fill('2');
  const pdfDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  const pdf = await pdfDownload;
  expect(new URL(pdf.url()).searchParams.get('filename')).toBe('合成钢琴练习 - v1.pdf');
  expect(pdf.suggestedFilename()).toBe('合成钢琴练习 - v1.pdf');
  expect(h.outputs).toHaveLength(1);
  expect(h.outputs[0]).toMatchObject({
    format: 'pdf',
    pages: [2],
    width: null,
    pdfPassword: 'secret',
    pngQuality: null,
    pngTransparent: null,
    source: { kind: 'version', versionId: olderVersionId },
  });
  await dialog.getByRole('checkbox', { name: '导出指定页面' }).uncheck();
  await expect(page.locator('.export-result')).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: /MIDI/ }).click();
  await expect(dialog.locator('.export-preview')).toBeVisible();
  await expect(dialog.locator('.export-preview-stage img')).toHaveCount(2);
  await dialog.getByRole('button', { name: /WAV/ }).click();
  await expect(dialog.locator('.export-preview')).toBeVisible();
  await expect(dialog.locator('.export-preview-stage img')).toHaveCount(2);
  const wavDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  expect((await wavDownload).suggestedFilename()).toBe('合成钢琴练习 - v1.wav');
  expect(h.outputs).toHaveLength(2);
  expect(h.outputs[1]).toMatchObject({
    format: 'wav',
    pages: null,
    width: null,
    pdfPassword: null,
    pngQuality: null,
    pngTransparent: null,
    source: { kind: 'version', versionId: olderVersionId },
  });
  await dialog.getByRole('button', { name: /PNG/ }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('有损压缩图')).toBeVisible();
  await expect(dialog.getByText('无损矢量图')).toBeVisible();
  await dialog.getByLabel('文件名').fill('版本导出');
  await dialog.getByLabel('像素宽度').fill('2400');
  const quality = dialog.getByRole('slider', { name: '图片质量' });
  await quality.focus();
  await page.keyboard.press('Home');
  await expect(quality).toHaveAttribute('aria-valuenow', '30');
  for (let value = 30; value < 72; value++) await page.keyboard.press('ArrowRight');
  await expect(quality).toHaveAttribute('aria-valuenow', '72');
  await expect(dialog.getByText('图片质量 72%')).toBeVisible();
  await dialog.getByLabel('透明背景').check();
  await expect(dialog.getByRole('checkbox', { name: '导出指定页面' })).not.toBeChecked();
  await expect(dialog.locator('.export-preview-stage img')).toHaveCount(2);
  const pngDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  expect((await pngDownload).suggestedFilename()).toBe('版本导出.zip');
  expect(h.outputs).toHaveLength(3);
  expect(h.outputs[2]).toMatchObject({
    format: 'png',
    pages: [1, 2],
    width: 2400,
    pdfPassword: null,
    pngQuality: 72,
    pngTransparent: true,
    source: { kind: 'version', versionId: olderVersionId },
  });
  await dialog.getByRole('button', { name: /SVG/ }).click();
  await dialog.getByRole('checkbox', { name: '导出指定页面' }).check();
  await dialog.getByLabel('起始页').fill('2');
  const svgDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  expect((await svgDownload).suggestedFilename()).toBe('版本导出.svg');
  expect(h.outputs[3]).toMatchObject({ format: 'svg', pages: [2] });
  await dialog.getByRole('button', { name: /PNG/ }).click();
  const selectedPng = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  expect((await selectedPng).suggestedFilename()).toBe('版本导出.png');
  expect(h.outputs[4]).toMatchObject({ format: 'png', pages: [2] });
  await expect(page.locator('.export-result')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '打开位置' })).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).not.toBeVisible();
  await expect(page.getByRole('main')).toBeFocused();
  expect(await dialog.evaluate((node) => getComputedStyle(node).flexDirection)).toBe('column');
  await page.getByRole('button', { name: '导出' }).click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: /PDF/ }).click();
  await expect(dialog.getByLabel('PDF 密码')).toHaveAttribute('type', 'password');
  await expect(dialog.getByLabel('PDF 密码')).toHaveValue('');
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).not.toBeVisible();
  await page.getByRole('button', { name: '恢复至此版本' }).click();
  await expect(page.getByRole('heading', { name: '恢复至此版本？' })).toBeVisible();
  const confirmDialog = page.locator('.confirm-dialog');
  await expect(confirmDialog.locator('.confirm-dialog-footer')).toBeVisible();
  await expect(confirmDialog).toHaveCSS('outline-style', 'none');
  await page.getByRole('button', { name: '确认恢复' }).click();
  await expect.poll(() => h.restores).toHaveLength(1);
  await expect(page.getByRole('main')).toBeFocused();
  expect(h.restores[0]).toMatchObject({
    versionId: olderVersionId,
    expectedSourceHash: h.states[0]!.sourceHash,
    discardCurrentEdit: true,
  });
});

test('export settings persist per score without saving the PDF password', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '导出' }).click();
  const dialog = page.locator('.export-dialog');
  await dialog.getByLabel('PDF 密码').fill('secret');
  await dialog.getByRole('button', { name: /PNG/ }).click();
  await dialog.getByLabel('文件名').fill('我的导出');
  await dialog.getByLabel('像素宽度').fill('2400');
  await dialog.getByLabel('透明背景').check();
  await dialog.getByLabel('包含绘画').uncheck();
  const quality = dialog.getByRole('slider', { name: '图片质量' });
  await quality.focus();
  await page.keyboard.press('Home');
  await expect(dialog.getByLabel('导出指定页面')).toBeEnabled();
  await dialog.getByLabel('导出指定页面').check();
  await dialog.getByLabel('起始页').fill('2');
  await dialog.getByRole('button', { name: '关闭' }).click();
  await page.reload();
  await page.getByRole('button', { name: '导出' }).click();
  await expect(dialog.getByRole('button', { name: /PNG/ })).toHaveAttribute('aria-pressed', 'true');
  await expect(dialog.getByLabel('文件名')).toHaveValue('我的导出');
  await expect(dialog.getByLabel('像素宽度')).toHaveValue('2400');
  await expect(dialog.getByLabel('透明背景')).toBeChecked();
  await expect(dialog.getByLabel('包含绘画')).not.toBeChecked();
  await expect(quality).toHaveAttribute('aria-valuenow', '30');
  await expect(dialog.getByLabel('导出指定页面')).toBeChecked();
  await expect(dialog.getByLabel('起始页')).toHaveValue('2');
  await dialog.getByLabel('结束页').fill('3');
  await expect(dialog.getByRole('button', { name: '导出', exact: true })).toBeDisabled();
  await dialog.getByRole('button', { name: /PDF/ }).click();
  await expect(dialog.getByLabel('PDF 密码')).toHaveValue('');
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain('secret');
  await page.goto(`/scores?documentId=${secondId}`);
  await page.getByRole('button', { name: '导出' }).click();
  await expect(dialog.getByRole('button', { name: /PDF/ })).toHaveAttribute('aria-pressed', 'true');
  await expect(dialog.getByLabel('文件名')).not.toHaveValue('我的导出');
});

test('export settings handle invalid or unavailable browser storage', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.evaluate((id) => localStorage.setItem(`ossia:export-settings:v1:${id}`, '{'), firstId);
  await page.reload();
  await page.getByRole('button', { name: '导出' }).click();
  const dialog = page.locator('.export-dialog');
  await expect(dialog.getByRole('button', { name: /PDF/ })).toHaveAttribute('aria-pressed', 'true');
  await page.evaluate(() => {
    Storage.prototype.setItem = () => {
      throw new Error('storage unavailable');
    };
  });
  await dialog.getByRole('button', { name: /PNG/ }).click();
  await dialog.getByLabel('像素宽度').fill('1200');
  await expect(dialog.getByLabel('像素宽度')).toHaveValue('1200');
});

test('export preview can recover from a page loading error', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  let fail = true;
  await page.route('**/renders/*/pages/*', async (route) => {
    if (fail) await route.fulfill({ status: 502, body: 'unavailable' });
    else await route.fallback();
  });
  await page.getByRole('button', { name: '导出', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '导出' });
  await expect(dialog.getByRole('alert')).toContainText('无法载入谱面');
  await expect(dialog.locator('.export-page-count')).toHaveCount(0);
  fail = false;
  await dialog.getByRole('button', { name: '重试' }).click();
  await expect(dialog.locator('.export-preview-stage img')).toHaveCount(2);
  await expect(dialog.getByText('第 1 页 / 共 2 页')).toBeVisible();
});

test('dirty workspace switches the version menu label while an agent edits', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  h.change();
  h.states[0]!.editState = 'active';
  h.emit('document-changed', firstId);
  const versionButton = page.locator('.score-version-control').getByRole('button');
  await expect(versionButton).toHaveText('正在编辑');
  await versionButton.click();
  const editingDraft = page.getByRole('menuitemradio', { name: /正在编辑/ });
  await expect(editingDraft).toContainText('正在编辑');
  await expect(editingDraft).toContainText('Ossia 正在编辑乐谱');
  await expect(editingDraft).not.toContainText('工作区有未提交修改');
  await expect(editingDraft).toHaveAttribute('aria-checked', 'true');
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await expect(versionButton).toHaveText('版本 1');
  await versionButton.click();
  await page.getByRole('menuitemradio', { name: /正在编辑/ }).click();
  await expect(versionButton).toHaveText('正在编辑');

  h.states[0]!.editState = 'idle';
  h.emit('document-changed', firstId);
  await expect(versionButton).toHaveText('当前草稿');
});

test('agent draft compilation preserves its last displayed score', async ({ context, page }) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);

  h.states[0]!.lastSuccessfulRender = null;
  h.setJob({
    jobId: 'render-job',
    documentId: firstId,
    kind: 'check',
    state: 'running',
    sourceHash: newer,
    phase: 'compiling',
    result: null,
    error: null,
  });
  h.change();

  await expect(page.locator('.score-version-control button')).toHaveText('当前草稿');
  await expect(page.locator('.score-loading-state')).toHaveCount(0);
  await expect(page.locator('.score-image')).toHaveCount(2);

  h.setJob(null);
  h.emit('job-changed', firstId, { jobId: 'render-job', state: 'error' });
  await expect(page.locator('.score-image')).toHaveCount(2);
  await expect(page.getByRole('heading', { name: '尚无可用谱面' })).toHaveCount(0);
});

test('active agent edits preserve their last displayed render', async ({ context, page }) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);

  h.states[0]!.lastSuccessfulRender = null;
  h.states[0]!.editState = 'active';
  h.emit('document-changed', firstId);

  await expect(page.locator('.score-view')).toHaveClass(/is-agent-editing/);
  await expect(page.locator('.score-loading-state')).toHaveCount(0);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await expect(page.getByRole('heading', { name: '尚无可用谱面' })).toHaveCount(0);
});

test('an active edit without a render or pending job does not keep loading', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context, { eventsDelay: 10000 });
  h.states[0]!.lastSuccessfulRender = null;
  h.states[0]!.editState = 'active';
  h.states[0]!.dirty = true;
  await page.goto(`/scores?documentId=${firstId}`);

  await expect(page.locator('.score-version-control button')).toHaveText('正在编辑');
  await expect(page.getByRole('heading', { name: '尚无可用谱面' })).toBeVisible();
  await expect(page.locator('.score-loading-state')).toHaveCount(0);
});

test('active agent edits disable playback, annotation and drawing tools', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const playback = page.locator('.score-playback-toggle');
  const annotation = page.getByRole('button', { name: '打开标注' });
  const pen = page.getByRole('button', { name: '画笔', exact: true });
  const eraser = page.getByRole('button', { name: '橡皮擦', exact: true });
  await expect(playback).toHaveAttribute('aria-disabled', 'false');
  await expect(annotation).toHaveAttribute('aria-disabled', 'false');
  await expect(pen).toHaveAttribute('aria-disabled', 'false');
  await expect(eraser).toHaveAttribute('aria-disabled', 'false');
  await pen.click();
  await expect(pen).toHaveAttribute('aria-pressed', 'true');

  h.states[0]!.editState = 'active';
  h.emit('document-changed', firstId);
  await expect(page.locator('.score-view')).toHaveClass(/is-agent-editing/);
  await expect(playback).toHaveAttribute('aria-disabled', 'true');
  await expect(annotation).toHaveAttribute('aria-disabled', 'true');
  await expect(pen).toHaveAttribute('aria-disabled', 'true');
  await expect(eraser).toHaveAttribute('aria-disabled', 'true');
  await expect(pen).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('.score-drawing-options')).toHaveCount(0);
  await expect(playback).toHaveAttribute('data-tooltip', '正在编辑乐谱，暂不可用');

  await page.keyboard.press('d');
  await page.keyboard.press('r');
  await page.keyboard.press('e');
  await expect(pen).toHaveAttribute('aria-pressed', 'false');
  await expect(annotation).toHaveAttribute('aria-pressed', 'false');

  h.states[0]!.editState = 'idle';
  h.emit('document-changed', firstId);
  await expect(page.locator('.score-view')).not.toHaveClass(/is-agent-editing/);
  await expect(playback).toHaveAttribute('aria-disabled', 'false');
  await expect(annotation).toHaveAttribute('aria-disabled', 'false');
  await expect(pen).toHaveAttribute('aria-disabled', 'false');
  await expect(eraser).toHaveAttribute('aria-disabled', 'false');
});

test('an idle event stream keeps score tools available', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const annotation = page.getByRole('button', { name: '打开标注' });
  await expect(annotation).toBeVisible();
  await page.evaluate(() => {
    Reflect.set(window, 'toolbarDisconnected', false);
    const toolbar = document.querySelector('.score-feature-toolbar')!;
    new MutationObserver(() => {
      if (!toolbar.isConnected) Reflect.set(window, 'toolbarDisconnected', true);
    }).observe(document, { childList: true, subtree: true });
  });
  await page.waitForTimeout(3_000);
  expect(await page.evaluate(() => Reflect.get(window, 'toolbarDisconnected'))).toBe(false);
  await expect(annotation).toBeVisible();
  await expect(annotation).toHaveAttribute('aria-disabled', 'false');
});

test('stale source clears selection without exposing diagnostics and reconnect works', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.keyboard.press('e');
  await expect(page.getByRole('button', { name: '关闭标注' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  const overlay = page.locator('.overlay').first();
  const box = await overlay.boundingBox();
  await overlay.click({ position: { x: box!.width * 0.4, y: (box!.height * 25) / 140 } });
  await page.getByRole('textbox', { name: '标注' }).fill('源码将变化');
  await page.getByRole('button', { name: '提交标注' }).click();
  await expect(page.locator('.annotation-box')).toHaveCount(1);
  h.change();
  await expect(page.locator('.annotation-box')).toHaveCount(0);
  await page.waitForTimeout(100);
  await expect(page.locator('.toast')).toHaveCount(0);
  await page.locator('.score-version-control button').click();
  await expect(page.getByText('上次成功结果：', { exact: false })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.locator('.diagnostics')).toHaveCount(0);
  h.disconnect(true);
  await expect(page.locator('.toast.is-error')).toBeVisible();
  await expect(page.locator('.message.error')).toHaveCount(0);
  const reconnected = page.waitForRequest('**/api/v1/events?*');
  h.disconnect(false);
  await reconnected;
  h.emit('ui-language-changed', firstId, { language: 'en-US' });
  await expect(page.getByRole('button', { name: 'Help' })).toBeVisible();
  await expect(page.getByText('已连接')).toHaveCount(0);
});

test('MIDI and WAV exports accept validated measure ranges', async ({ context, page }) => {
  const playback: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 6,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 1,
      },
    ],
    groups: [],
    systems: [
      {
        page: 1,
        systemIndex: 0,
        measureStart: 1,
        measureEnd: 3,
        startSeconds: 0,
        endSeconds: 6,
        x0: 10,
        x1: 90,
        y0: 18,
        y1: 32,
      },
    ],
  };
  const h = await mockScoreRuntime(context, { playback });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '导出' }).click();
  const dialog = page.locator('.export-dialog');
  await dialog.getByRole('button', { name: /MIDI/ }).click();
  const range = dialog.getByLabel('导出指定小节');
  await expect(range).toBeEnabled();
  await range.check();
  await dialog.getByLabel('起始小节').fill('2');
  await dialog.getByLabel('结束小节').fill('3');

  const midiDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  await midiDownload;
  expect(h.outputs[0]).toMatchObject({
    format: 'midi',
    audioRange: { start: 2, end: 3 },
  });

  await dialog.getByLabel('结束小节').fill('4');
  await expect(dialog.getByRole('button', { name: '导出', exact: true })).toBeDisabled();
  await dialog.getByLabel('结束小节').fill('3');

  await dialog.getByRole('button', { name: /WAV/ }).click();
  await expect(range).toBeEnabled();
  await expect(range).toBeChecked();
  const wavDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  await wavDownload;
  expect(h.outputs[1]).toMatchObject({
    format: 'wav',
    audioRange: { start: 2, end: 3 },
  });
  await dialog.getByRole('button', { name: /PDF/ }).click();
  const pdfDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  await pdfDownload;
  expect(h.outputs[2]).toMatchObject({ format: 'pdf', audioRange: null });
  await dialog.getByRole('button', { name: /WAV/ }).click();
  await expect(range).toBeChecked();
  await page.reload();
  await page.getByRole('button', { name: '导出' }).click();
  await expect(range).toBeChecked();
  await expect(dialog.getByLabel('起始小节')).toHaveValue('2');
  await expect(dialog.getByLabel('结束小节')).toHaveValue('3');
  await dialog.getByLabel('结束小节').fill('4');
  await page.reload();
  await page.getByRole('button', { name: '导出' }).click();
  await expect(dialog.getByLabel('结束小节')).toHaveValue('4');
  await expect(dialog.getByRole('button', { name: '导出', exact: true })).toBeDisabled();
  await dialog.getByLabel('结束小节').fill('3');
  await expect(dialog.getByRole('button', { name: '导出', exact: true })).toBeEnabled();
});
