import { ChevronDown } from 'lucide-react';
import { useEffect, useState } from 'react';

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
import type { Api } from '../api.ts';

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

  useEffect(() => {
    if (!open) return;
    let live = true;
    setLoading(true);
    setError('');
    api
      .documents()
      .then((items) => {
        if (live) setDocuments(items);
      })
      .catch((reason) => {
        if (live) setError(describe(reason));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [api, attempt, open]);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="score-title-trigger"
          aria-label={`${t('chooseScore')}: ${title}`}
          title={title}
        >
          <span className="topbar-title">{title}</span>
          <ChevronDown className="score-title-chevron" aria-hidden="true" size={15} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="score-title-menu">
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
          <DropdownMenuItem disabled>{t('loading')}</DropdownMenuItem>
        ) : null}
        {error && (
          <>
            <p className="score-title-error" role="alert">
              {t('scoreListFailed', { error })}
            </p>
            <DropdownMenuItem onSelect={() => setAttempt((value) => value + 1)}>
              {t('retry')}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
