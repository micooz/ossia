import type {
  DocumentState,
  Locator,
  RenderManifest,
  Version,
  ViewSnapshot,
} from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { Api } from '../api.ts';
import { locatorKey } from '../api.ts';
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
  const { t } = useI18n();
  const measureLabel = (locator: Locator) =>
    locator.measureEnd === locator.measureStart
      ? t('measure', { start: locator.measureStart })
      : t('measureRange', {
          start: locator.measureStart,
          end: locator.measureEnd,
        });
  return (
    <aside className="details-column">
      <section className="panel selection">
        <h2>{t('selectionTitle')}</h2>
        <p className="hint">{t('selectionDescription')}</p>
        {!selected.length ? (
          <p>{t('selectionEmpty')}</p>
        ) : (
          <ul>
            {selected.map((l) => (
              <li key={locatorKey(l)}>
                {t('staff', { staff: l.staffIndex + 1 })} · {measureLabel(l)}
              </li>
            ))}
          </ul>
        )}
        <button type="button" disabled={!selected.length || !connected || stale} onClick={onClear}>
          {t('clearSelection')}
        </button>
      </section>
      <section className="panel diagnostics">
        <h2>{t('diagnosticsTitle')}</h2>
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
  const { t } = useI18n();
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
              {d.position &&
                t('lineColumn', {
                  line: d.position.line,
                  column: d.position.column,
                })}
              {d.message}
            </li>
          ))}
        </ul>
      ) : (
        <p className="hint">
          {doc.lastCheck?.state === 'error' ? t('diagnosticsFallback') : t('noDiagnostics')}
        </p>
      )}
      {logs.length > 0 && (
        <details>
          <summary>{t('showCompileLog')}</summary>
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
