import { Check, ChevronDown, Copy, LoaderCircle, ScrollText } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { Button } from '../../components/ui/button.tsx';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu.tsx';
import { describe } from '../../errors.ts';
import type { DocumentSummary } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { Api } from '../utils/api.ts';
import { copyText } from '../utils/clipboard.ts';

function byUpdatedAtDescending(left: DocumentSummary, right: DocumentSummary) {
  return Date.parse(right.updatedAt) - Date.parse(left.updatedAt);
}

export function ScoreTitleMenu({
  api,
  documentId,
  title,
  onSelect,
}: {
  api: Api;
  documentId: string;
  title: string;
  onSelect: (documentId: string) => void;
}) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [documents, setDocuments] = useState<DocumentSummary[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const [copyStatus, setCopyStatus] = useState<{ kind: 'success' | 'error' } | null>(null);
  const pointerSelection = useRef(false);
  const penPress = useRef<{ id: number; x: number; y: number; item: HTMLElement } | null>(null);
  const loadedDocuments = useRef<DocumentSummary[] | null>(null);

  useEffect(() => {
    if (!copyStatus) return;
    const timer = window.setTimeout(() => setCopyStatus(null), 2000);
    return () => window.clearTimeout(timer);
  }, [copyStatus]);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setLoading(true);
    setError('');
    api
      .documents()
      .then((items) => {
        if (live) {
          const sorted = items.toSorted(byUpdatedAtDescending);
          loadedDocuments.current = sorted;
          setDocuments(sorted);
        }
      })
      .catch((reason) => {
        if (live && loadedDocuments.current === null) setError(describe(reason));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [api, attempt, open]);

  return (
    <div className="score-title-selector">
      <DropdownMenu
        open={open}
        onOpenChange={(next) => {
          if (next) pointerSelection.current = false;
          setOpen(next);
        }}
      >
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="score-title-trigger score-selector-trigger"
            aria-label={`${t('chooseScore')}: ${title}`}
            title={title}
          >
            <ScrollText
              className="score-title-menu-icon score-selector-menu-icon"
              aria-hidden="true"
              size={17}
            />
            <span className="topbar-title score-title-label score-selector-label">{title}</span>
            <ChevronDown
              className="score-title-chevron score-selector-chevron"
              aria-hidden="true"
              size={15}
            />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          className="score-title-menu"
          onPointerDown={(event) => {
            pointerSelection.current = true;
            const item = (event.target as HTMLElement).closest<HTMLElement>(
              '[role="menuitemradio"]',
            );
            penPress.current =
              event.pointerType === 'pen' && event.button === 0 && item
                ? { id: event.pointerId, x: event.clientX, y: event.clientY, item }
                : null;
          }}
          onPointerMove={(event) => {
            const press = penPress.current;
            if (
              press?.id === event.pointerId &&
              Math.hypot(event.clientX - press.x, event.clientY - press.y) > 10
            ) {
              penPress.current = null;
            }
          }}
          onPointerCancel={() => {
            penPress.current = null;
          }}
          onPointerUp={(event) => {
            const press = penPress.current;
            penPress.current = null;
            if (
              press?.id === event.pointerId &&
              Math.hypot(event.clientX - press.x, event.clientY - press.y) <= 10 &&
              press.item.contains(document.elementFromPoint(event.clientX, event.clientY))
            ) {
              // Safari may omit click after a longer Pencil contact.
              event.preventDefault();
              press.item.click();
            }
          }}
          onCloseAutoFocus={(event) => {
            if (!pointerSelection.current) return;
            event.preventDefault();
            pointerSelection.current = false;
          }}
        >
          <DropdownMenuLabel>{t('chooseScore')}</DropdownMenuLabel>
          {documents ? (
            <DropdownMenuRadioGroup
              value={documentId}
              onValueChange={(next) => {
                if (next !== documentId) onSelect(next);
              }}
            >
              {documents.map((document) => (
                <DropdownMenuRadioItem
                  key={document.documentId}
                  value={document.documentId}
                  title={document.title}
                >
                  <span className="ui-menu-item-title">{document.title}</span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          ) : loading ? (
            <div className="score-title-loading-item" role="status">
              <LoaderCircle className="score-title-loading-spinner" aria-hidden="true" size={15} />
              <span>{t('loading')}</span>
            </div>
          ) : null}
          {error && (
            <>
              <p className="score-title-error" role="alert">
                {t('scoreListFailed', { error })}
              </p>
              <DropdownMenuItem
                className="score-title-retry-item"
                onSelect={() => setAttempt((value) => value + 1)}
              >
                <span className="ui-menu-item-title">{t('retry')}</span>
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <button
        type="button"
        className="score-title-copy"
        aria-label={t('copyScoreName')}
        onClick={async () => {
          try {
            await copyText(title);
            setCopyStatus({ kind: 'success' });
          } catch {
            setCopyStatus({ kind: 'error' });
          }
        }}
      >
        {copyStatus?.kind === 'success' ? (
          <Check aria-hidden="true" size={17} />
        ) : (
          <Copy aria-hidden="true" size={17} />
        )}
      </button>
      <span
        className="score-title-copy-status"
        data-success={copyStatus?.kind === 'success'}
        role="status"
      >
        {copyStatus?.kind === 'success'
          ? t('scoreNameCopied')
          : copyStatus?.kind === 'error'
            ? t('copyScoreNameFailed')
            : ''}
      </span>
    </div>
  );
}
