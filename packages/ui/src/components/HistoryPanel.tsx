import type { Version } from '../generated/protocol.ts';
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
  return (
    <section className="panel history">
      <h2>版本历史</h2>
      {!versions.length && <p className="hint">尚无已提交版本</p>}
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
                版本 {v.seq}
                {head === v.versionId ? ' · 当前版本' : ''}
              </strong>
              <span>{v.summary || v.origin}</span>
              <small>
                {v.author} · {new Date(v.createdAt).toLocaleString('zh-CN')}
              </small>
            </button>
            <button
              type="button"
              disabled={busy || head === v.versionId}
              onClick={() => onRestore(v)}
            >
              恢复至版本 {v.seq}
            </button>
          </li>
        ))}
      </ol>
    </section>
  );
}
