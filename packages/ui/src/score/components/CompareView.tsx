import { useEffect, useState } from 'react';

import { describe } from '../../errors.ts';
import type { Locator, RenderReference, Version } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { Api } from '../api.ts';
import type { ScoreLayout } from './ScoreToolbar.tsx';
import { ScoreView } from './ScoreView.tsx';

function VersionScore({
  api,
  documentId,
  versionId,
  layout,
  onLayoutChange,
  zoom,
  onZoomChange,
}: {
  api: Api;
  documentId: string;
  versionId: string;
  layout: ScoreLayout;
  onLayoutChange: (layout: ScoreLayout) => void;
  zoom: number;
  onZoomChange: (zoom: number) => void;
}) {
  const { t } = useI18n();
  const [render, setRender] = useState<RenderReference | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let live = true;
    setRender(null);
    setError('');
    api
      .renderVersion(documentId, versionId)
      .then((job) => api.waitJob<RenderReference>(job))
      .then((value) => {
        if (live) setRender(value);
      })
      .catch((reason) => {
        if (live) setError(describe(reason));
      });
    return () => {
      live = false;
    };
  }, [api, documentId, versionId]);
  return error ? (
    <p className="error" role="alert">
      {error}
    </p>
  ) : render ? (
    <ScoreView
      api={api}
      render={render}
      layout={layout}
      onLayoutChange={onLayoutChange}
      zoom={zoom}
      onZoomChange={onZoomChange}
    />
  ) : (
    <p className="score-loading">{t('loadingHistoryScore')}</p>
  );
}

export function CompareView({
  api,
  documentId,
  versions,
  leftVersionId,
  rightVersionId,
  currentRender,
  layout,
  onLayoutChange,
  leftZoom,
  onLeftZoom,
  rightZoom,
  onRightZoom,
  selected,
  selectable,
  onPoint,
}: {
  api: Api;
  documentId: string;
  versions: Version[];
  leftVersionId: string | null;
  rightVersionId: string;
  currentRender: RenderReference | null;
  layout: ScoreLayout;
  onLayoutChange: (layout: ScoreLayout) => void;
  leftZoom: number;
  onLeftZoom: (zoom: number) => void;
  rightZoom: number;
  onRightZoom: (zoom: number) => void;
  selected: Locator[];
  selectable: boolean;
  onPoint: (page: number, x: number, y: number) => void;
}) {
  const { t } = useI18n();
  const leftVersion = versions.find((version) => version.versionId === leftVersionId);
  const rightVersion = versions.find((version) => version.versionId === rightVersionId);
  return (
    <section className="compare" aria-label={t('compareTitle')}>
      <div className="compare-pane">
        <div className="pane-bar">
          <strong>{leftVersion ? `${t('version')} ${leftVersion.seq}` : t('currentScore')}</strong>
        </div>
        <div className="pane-body">
          {leftVersionId ? (
            <VersionScore
              api={api}
              documentId={documentId}
              versionId={leftVersionId}
              layout={layout}
              onLayoutChange={onLayoutChange}
              zoom={leftZoom}
              onZoomChange={onLeftZoom}
            />
          ) : currentRender ? (
            <ScoreView
              api={api}
              render={currentRender}
              layout={layout}
              onLayoutChange={onLayoutChange}
              zoom={leftZoom}
              onZoomChange={onLeftZoom}
              selected={selected}
              selectable={selectable}
              onPoint={onPoint}
            />
          ) : (
            <p className="score-loading">{t('noRenderTitle')}</p>
          )}
        </div>
      </div>
      <div className="compare-pane">
        <div className="pane-bar">
          <strong>
            {rightVersion ? `${t('version')} ${rightVersion.seq}` : t('loadingHistoryScore')}
          </strong>
        </div>
        <div className="pane-body">
          <VersionScore
            api={api}
            documentId={documentId}
            versionId={rightVersionId}
            layout={layout}
            onLayoutChange={onLayoutChange}
            zoom={rightZoom}
            onZoomChange={onRightZoom}
          />
        </div>
      </div>
    </section>
  );
}

export { VersionScore };
