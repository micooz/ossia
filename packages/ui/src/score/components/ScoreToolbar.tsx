import { Download, GitCompareArrows, History, RotateCcw } from 'lucide-react';
import { useState } from 'react';

import { Button } from '../../components/ui/button.tsx';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu.tsx';
import type { RenderReference, Version } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { Api } from '../api.ts';
import { ExportDialog } from './ExportDialog.tsx';

export type ScoreLayout = 'vertical' | 'horizontal';

export function ScoreToolbar({
  api,
  versions,
  viewingVersionId,
  onViewVersion,
  compareVersionId,
  onCompare,
  documentTitle,
  currentRender,
  documentHint,
  exportVersionId,
  exportDisabled,
  onRestore,
}: {
  api: Api;
  versions: Version[];
  viewingVersionId: string | null;
  onViewVersion: (versionId: string | null) => void;
  compareVersionId: string | null;
  onCompare: (versionId: string | null) => void;
  documentTitle: string;
  currentRender: RenderReference | null;
  documentHint: string;
  exportVersionId: string | null;
  exportDisabled: boolean;
  onRestore?: (version: Version) => void;
}) {
  const { t } = useI18n();
  const [exportOpen, setExportOpen] = useState(false);
  const activeVersion = versions.find((version) => version.versionId === viewingVersionId);
  const compareVersion = versions.find((version) => version.versionId === compareVersionId);
  return (
    <>
      <div className="score-actions">
        {activeVersion && onRestore && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label={t('restoreVersion', { version: activeVersion.seq })}
            title={t('restoreVersion', { version: activeVersion.seq })}
            onClick={() => onRestore(activeVersion)}
          >
            <RotateCcw aria-hidden="true" size={17} />
          </Button>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t('versionMenu')}
              title={activeVersion ? `${t('version')} ${activeVersion.seq}` : t('currentScore')}
            >
              <History aria-hidden="true" size={17} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuRadioGroup
              value={viewingVersionId ?? 'current'}
              onValueChange={(value) => onViewVersion(value === 'current' ? null : value)}
            >
              <DropdownMenuRadioItem value="current">
                <span className="ui-menu-item-title">{t('currentScore')}</span>
                <span className="ui-menu-item-description">{documentHint}</span>
              </DropdownMenuRadioItem>
              {versions.map((version) => (
                <DropdownMenuRadioItem key={version.versionId} value={version.versionId}>
                  <span className="ui-menu-item-title">
                    {t('version')} {version.seq}
                  </span>
                  <span className="ui-menu-item-description">
                    {version.summary || version.origin}
                  </span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t('compareMenu')}
              aria-pressed={!!compareVersion}
              title={
                compareVersion
                  ? `${t('compareMenu')} · ${t('version')} ${compareVersion.seq}`
                  : t('compareMenu')
              }
            >
              <GitCompareArrows aria-hidden="true" size={17} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {compareVersionId && (
              <>
                <DropdownMenuItem onSelect={() => onCompare(null)}>
                  <span className="ui-menu-item-title">{t('exitCompare')}</span>
                </DropdownMenuItem>
                <DropdownMenuSeparator />
              </>
            )}
            <DropdownMenuRadioGroup
              value={compareVersionId ?? ''}
              onValueChange={(value) => {
                if (value) onCompare(value);
              }}
            >
              <DropdownMenuRadioItem value="current" disabled>
                <span className="ui-menu-item-title">
                  {activeVersion
                    ? `${t('version')} ${activeVersion.seq} · ${t('current')}`
                    : `${t('currentScore')} · ${t('current')}`}
                </span>
                <span className="ui-menu-item-description">
                  {activeVersion ? activeVersion.summary || activeVersion.origin : documentHint}
                </span>
              </DropdownMenuRadioItem>
              <DropdownMenuSeparator />
              {versions
                .filter((version) => version.versionId !== viewingVersionId)
                .map((version) => (
                  <DropdownMenuRadioItem key={version.versionId} value={version.versionId}>
                    <span className="ui-menu-item-title">
                      {t('version')} {version.seq}
                    </span>
                    <span className="ui-menu-item-description">
                      {version.summary || version.origin}
                    </span>
                  </DropdownMenuRadioItem>
                ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label={t('exportAction')}
          title={t('exportAction')}
          disabled={exportDisabled || !exportVersionId}
          onClick={() => setExportOpen(true)}
        >
          <Download aria-hidden="true" size={17} />
        </Button>
      </div>
      <ExportDialog
        api={api}
        open={exportOpen}
        onClose={() => setExportOpen(false)}
        documentTitle={documentTitle}
        viewingVersion={activeVersion ?? null}
        exportVersionId={exportVersionId}
        currentRender={currentRender}
      />
    </>
  );
}
