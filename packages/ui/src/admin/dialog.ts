import type { SyntheticEvent } from 'react';

let topLayerExit: boolean | undefined;

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
