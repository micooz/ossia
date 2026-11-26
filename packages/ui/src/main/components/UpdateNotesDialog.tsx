import { useEffect, useRef } from 'react';

import type { UiLanguage, UpdateNotes } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { closeDialog } from '../utils/dialog.ts';
import { renderReleaseNotes, secureLinks } from '../utils/releaseNotes.ts';

export function UpdateNotesDialog({
  notes,
  language,
  onClose,
  onOpenLink,
}: {
  notes: UpdateNotes;
  language: UiLanguage;
  onClose: () => void;
  onOpenLink: (url: string) => void;
}) {
  const { t } = useI18n();
  const dialog = useRef<HTMLDialogElement>(null);
  const content = useRef<HTMLDivElement>(null);

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  useEffect(() => {
    const root = content.current;
    if (!root) return;
    if (notes.maintenanceOnly) {
      const notice = document.createElement('p');
      notice.className = 'release-notes-maintenance';
      notice.textContent = t('updateMaintenanceOnly');
      root.replaceChildren(notice);
    } else {
      renderReleaseNotes(root, notes, language, t('updateOtherChanges'));
    }
  }, [notes, language, t]);

  useEffect(() => {
    const root = content.current;
    if (!root) return;
    secureLinks(root);
    const click = (event: MouseEvent) => {
      const link = (event.target as Element | null)?.closest<HTMLAnchorElement>('a[href]');
      if (!link) return;
      event.preventDefault();
      onOpenLink(link.href);
    };
    root.addEventListener('click', click);
    return () => root.removeEventListener('click', click);
  }, [notes, onOpenLink]);

  const close = () => {
    if (dialog.current) void closeDialog(dialog.current).then(onClose);
    else onClose();
  };
  const version = (value: string) => value.replace(/^v/, '');

  return (
    <dialog
      ref={dialog}
      className="service-dialog update-notes-dialog"
      aria-labelledby="update-notes-title"
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
    >
      <div className="update-notes-header">
        <h2 id="update-notes-title" tabIndex={-1} autoFocus>
          {t('updateNotesTitle', { version: version(notes.to) })}
        </h2>
        {version(notes.from) !== version(notes.to) && (
          <p className="update-notes-range">
            v{version(notes.from)} → v{version(notes.to)}
          </p>
        )}
      </div>
      <div className="update-notes-content release-notes-content" ref={content} />
      <footer className="update-notes-footer">
        <button className="update-notes-close" type="button" onClick={close}>
          {t('close')}
        </button>
      </footer>
    </dialog>
  );
}
