// Chain 4: the version chain, plus restore. A restore is a new node that points at old content, so
// the chain stays linear and nothing is deleted (ADR-0001 §6.4). `isHead` is the tip node, not
// "hash equals head_hash" — a restore legitimately reuses an older hash.
import type { DocumentDetail } from '../api.ts';

interface Props {
  doc: DocumentDetail;
  currentHash: string;
  busy: boolean;
  onOpen: (hash: string) => void;
  onRestore: (targetHash: string) => void;
}

const ORIGIN: Record<string, string> = {
  import: '导入',
  agent: 'AI 修改',
  restore: '回退',
};

export function HistoryPanel({ doc, currentHash, busy, onOpen, onRestore }: Props) {
  return (
    <section className="panel history">
      <h3>历史版本</h3>
      <ol className="chain">
        {doc.versions.map((v) => (
          <li key={`${v.seq}:${v.hash}`} className={v.hash === currentHash ? 'on' : ''}>
            <button type="button" className="seq" onClick={() => onOpen(v.hash)} title="查看该版本">
              #{v.seq}
            </button>
            <div className="meta">
              <span className={v.isHead ? 'tag head' : 'tag'}>{ORIGIN[v.origin] ?? v.origin}</span>
              {v.isHead && <span className="tag head">当前</span>}
              <span className="hint">{new Date(v.createdAt).toLocaleString()}</span>
              <span className="hint mono">{v.hash.slice(0, 8)}</span>
            </div>
            <div className="meta sub">
              <span className="hint">{v.author}</span>
              {v.preview && (
                <span className="hint">
                  {v.preview.pageCount} 页 · {Math.round(v.preview.ms)} ms
                </span>
              )}
              {!v.preview && <span className="hint">未渲染</span>}
              {!v.isHead && (
                <button type="button" disabled={busy} onClick={() => onRestore(v.hash)}>
                  回退到此版本
                </button>
              )}
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}
