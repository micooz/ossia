import { History, RotateCcw } from 'lucide-react';

import type { Version } from '../generated/protocol.ts';
import { formatDate, useI18n } from '../i18n.tsx';
export function HistoryPanel({
  versions,
  head,
  selected,
  busy,
  onSelect,
  onRestore,
}: {
  versions: Version[];
  head: string | null;
  selected: string | null;
  busy: boolean;
  onSelect: (id: string) => void;
  onRestore: (version: Version) => void;
}) {
  const { language, t } = useI18n();
  return (
    <section className="panel history">
      <h2>
        <History aria-hidden="true" size={17} /> {t('historyTitle')}
      </h2>
      {!versions.length && <p className="hint">{t('noVersions')}</p>}
      <ol className="version-list">
        {versions.map((v) => (
          <li key={v.versionId}>
            <button
              type="button"
              className={selected === v.versionId ? 'version on' : 'version'}
              disabled={busy}
              onClick={() => onSelect(v.versionId)}
            >
              <strong>
                {t('version')} {v.seq}
                {head === v.versionId ? ` · ${t('currentVersion')}` : ''}
              </strong>
              <span>{v.summary || v.origin}</span>
              <small>
                {v.author} · {formatDate(language, v.createdAt)}
              </small>
            </button>
            <button
              type="button"
              disabled={busy || head === v.versionId}
              onClick={() => onRestore(v)}
            >
              <RotateCcw aria-hidden="true" size={14} />
              {t('restoreVersion', { version: v.seq })}
            </button>
          </li>
        ))}
      </ol>
    </section>
  );
}
