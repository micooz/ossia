import type { SyntheticEvent } from 'react';

import { registerDialogFallback } from '../../polyfills.ts';

import 'dialog-polyfill/dist/dialog-polyfill.css';

let topLayerExit: boolean | undefined;
const fallbackFocus = new WeakMap<HTMLDialogElement, HTMLElement>();

/** Open a modal on both native and older WebViews. */
export function openDialog(dialog: HTMLDialogElement, invoker?: HTMLElement) {
  if (registerDialogFallback(dialog)) {
    dialog.setAttribute('aria-modal', 'true');
    dialog.addEventListener('close', () => {
      const previousFocus = fallbackFocus.get(dialog);
      fallbackFocus.delete(dialog);
      if (previousFocus) window.setTimeout(() => previousFocus.focus(), 0);
    });
  }
  const focused = invoker ?? document.activeElement;
  if (dialog.getAttribute('aria-modal') === 'true' && focused instanceof HTMLElement) {
    fallbackFocus.set(dialog, focused);
  }
  dialog.showModal();
}

function supportsTopLayerExit() {
  if (topLayerExit !== undefined) return topLayerExit;
  if (!CSS.supports('overlay', 'auto') || !CSS.supports('transition-behavior', 'allow-discrete')) {
    return (topLayerExit = false);
  }
  const probe = document.createElement('div');
  probe.style.cssText = 'transition: display 1s allow-discrete !important; display: block;';
  document.body.append(probe);
  void getComputedStyle(probe).display;
  probe.style.display = 'none';
  topLayerExit = getComputedStyle(probe).display === 'block';
  probe.remove();
  return topLayerExit;
}

/** Close a modal after its exit transition on WebViews without top-layer transitions. */
export async function closeDialog(dialog: HTMLDialogElement) {
  if (!dialog.open || dialog.hasAttribute('data-closing')) return;
  if (!supportsTopLayerExit()) {
    dialog.setAttribute('data-closing', '');
    const animations = dialog.getAnimations();
    if (animations.length > 0) {
      let timer = 0;
      await Promise.race([
        Promise.allSettled(animations.map((animation) => animation.finished)),
        new Promise<void>((resolve) => {
          timer = window.setTimeout(resolve, 2000);
        }),
      ]);
      window.clearTimeout(timer);
    }
    dialog.close();
    dialog.removeAttribute('data-closing');
    return;
  }
  dialog.close();
}

/** Preserve the native Esc behavior where CSS can animate a native close. */
export function cancelDialog(event: SyntheticEvent<HTMLDialogElement>) {
  if (supportsTopLayerExit()) return;
  event.preventDefault();
  void closeDialog(event.currentTarget);
}
