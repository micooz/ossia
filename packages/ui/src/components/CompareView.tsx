import { useEffect, useState } from 'react';

import { describe } from '../api.ts';
import type { Api } from '../api.ts';
import type { RenderReference, Version } from '../generated/protocol.ts';
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
    <p className="hint">载入历史谱面…</p>
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
  const [left, setLeft] = useState(versions[1]?.versionId ?? versions[0]?.versionId ?? '');
  const [right, setRight] = useState(versions[0]?.versionId ?? '');
  return (
    <section className="compare" aria-label="版本对比">
      {[
        { label: '左侧版本', value: left, set: setLeft },
        { label: '右侧版本', value: right, set: setRight },
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
                  版本 {v.seq} · {v.summary || v.origin}
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
