import { useEffect, useState } from 'react';

import { describe } from '../api.ts';
import type { Api } from '../api.ts';
import type { RenderReference, Version } from '../generated/protocol.ts';
import { useI18n } from '../i18n.tsx';
import { ScoreView } from './ScoreView.tsx';

function VersionScore({
  api,
  documentId,
  versionId,
}: {
  api: Api;
  documentId: string;
  versionId: string;
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
      .then((j) => api.waitJob<RenderReference>(j))
      .then((r) => {
        if (live) setRender(r);
      })
      .catch((e) => {
        if (live) setError(describe(e));
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
    <ScoreView api={api} render={render} />
  ) : (
    <p className="hint">{t('loadingHistoryScore')}</p>
  );
}
export function CompareView({
  api,
  documentId,
  versions,
}: {
  api: Api;
  documentId: string;
  versions: Version[];
}) {
  const { t } = useI18n();
  const [left, setLeft] = useState(versions[1]?.versionId ?? versions[0]?.versionId ?? '');
  const [right, setRight] = useState(versions[0]?.versionId ?? '');
  return (
    <section className="compare" aria-label={t('compareTitle')}>
      {[
        { label: t('leftVersion'), value: left, set: setLeft },
        { label: t('rightVersion'), value: right, set: setRight },
      ].map((side) => (
        <div key={side.label}>
          <label>
            {side.label}{' '}
            <select
              aria-label={side.label}
              value={side.value}
              onChange={(e) => side.set(e.target.value)}
            >
              {versions.map((v) => (
                <option key={v.versionId} value={v.versionId}>
                  {t('version')} {v.seq} · {v.summary || v.origin}
                </option>
              ))}
            </select>
          </label>
          {side.value && <VersionScore api={api} documentId={documentId} versionId={side.value} />}
        </div>
      ))}
    </section>
  );
}
export { VersionScore };
