import type { Api } from '../api.ts';
import { locatorKey } from '../api.ts';
import type {
  DocumentState,
  Locator,
  RenderManifest,
  Version,
  ViewSnapshot,
} from '../generated/protocol.ts';
import { ExportBar } from './ExportBar.tsx';

export function DocumentDetails({
  api,
  doc,
  render,
  versions,
  selected,
  stale,
  connected,
  onClear,
  onError,
}: {
  api: Api;
  doc: DocumentState;
  render: ViewSnapshot['render'] | undefined;
  versions: Version[];
  selected: Locator[];
  stale: boolean;
  connected: boolean;
  onClear: () => void;
  onError: (error: unknown) => void;
}) {
  return (
    <aside className="details-column">
      <section className="panel selection">
        <h2>当前选区</h2>
        <p className="hint">选择会关联到当前视图，由 Agent 读取。</p>
        {!selected.length ? (
          <p>尚未选择小节</p>
        ) : (
          <ul>
            {selected.map((l) => (
              <li key={locatorKey(l)}>
                谱表 {l.staffIndex + 1} · 第 {l.measureStart}
                {l.measureEnd !== l.measureStart ? `–${l.measureEnd}` : ''} 小节
              </li>
            ))}
          </ul>
        )}
        <button type="button" disabled={!selected.length || !connected || stale} onClick={onClear}>
          清空选区
        </button>
      </section>
      <section className="panel diagnostics">
        <h2>编译诊断</h2>
        {doc.lastCheck?.error && (
          <p className="error" role="alert">
            {doc.lastCheck.error.message}
          </p>
        )}
        <Diagnostics doc={doc} render={render} stale={stale} />
      </section>
      {!!versions.length && (
        <ExportBar
          key={doc.documentId + (doc.headVersionId ?? '')}
          api={api}
          documentId={doc.documentId}
          versions={versions}
          onError={onError}
        />
      )}
    </aside>
  );
}

function Diagnostics({
  doc,
  render,
  stale,
}: {
  doc: DocumentState;
  render: ViewSnapshot['render'] | undefined;
  stale: boolean;
}) {
  const result = doc.lastCheck?.result as RenderManifest | null;
  const diagnostics = result?.diagnostics ?? (stale ? [] : (render?.manifest.diagnostics ?? []));
  const issues = diagnostics.filter((d) => d.severity !== 'info');
  const logs = diagnostics.filter((d) => d.severity === 'info');
  return (
    <>
      {issues.length ? (
        <ul>
          {issues.map((d, i) => (
            <li key={i}>
              {d.position && `第 ${d.position.line} 行，第 ${d.position.column} 列：`}
              {d.message}
            </li>
          ))}
        </ul>
      ) : (
        <p className="hint">
          {doc.lastCheck?.state === 'error'
            ? '请让 Agent 检查源码和任务详情。'
            : '暂无编译警告或错误'}
        </p>
      )}
      {logs.length > 0 && (
        <details>
          <summary>查看编译日志</summary>
          <ul>
            {logs.map((d, i) => (
              <li key={i}>{d.message}</li>
            ))}
          </ul>
        </details>
      )}
    </>
  );
}
