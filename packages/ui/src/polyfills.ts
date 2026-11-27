import 'core-js/actual/array/at';
import 'core-js/actual/string/at';
import dialogPolyfill from 'dialog-polyfill';

export function registerDialogFallback(dialog: HTMLDialogElement) {
  if (typeof dialog.showModal === 'function') return false;
  dialogPolyfill.registerDialog(dialog);
  dialog.setAttribute('data-dialog-fallback', '');
  return true;
}
