// Chain 1 in the UI: pick a file, and show what the converter reported instead of pretending the
// import was clean. The server returns 422 `confirm-required` for localised losses (ADR-0001 §6.1),
// so the player sees the exact warning list and decides; `Negative skip` is a hard refusal with no
// confirm path, because §3.11 measured it as a timeline corruption that confirmation cannot cure.
import { useState } from 'react';

import type { Api, ImportResult, ImportStats } from '../api.ts';
import { ApiError } from '../api.ts';

interface Props {
  api: Api;
  onImported: (r: ImportResult) => void;
  onBusy: (b: boolean) => void;
}

export function ImportPanel({ api, onImported, onBusy }: Props) {
  const [file, setFile] = useState<File | null>(null);
  const [report, setReport] = useState<ImportStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  async function send(confirm?: string) {
    if (!file) return;
    setWorking(true);
    onBusy(true);
    setError(null);
    try {
      const r = await api.importDocument(file, confirm);
      setReport(null);
      onImported(r);
    } catch (e) {
      if (e instanceof ApiError && (e.code === 'confirm-required' || e.code === 'needs-review')) {
        setReport(e.detail as ImportStats);
      } else setError(String((e as Error).message));
    } finally {
      setWorking(false);
      onBusy(false);
    }
  }

  return (
    <section className="panel import">
      <h3>导入 MusicXML / MIDI</h3>
      <input
        type="file"
        accept=".xml,.musicxml,.mxl,.mid,.midi"
        aria-label="选择乐谱文件"
        onChange={(ev) => {
          setFile(ev.target.files?.[0] ?? null);
          setReport(null);
          setError(null);
        }}
      />
      <div className="row">
        <button type="button" disabled={!file || working} onClick={() => void send()}>
          {working ? '导入中…' : '导入'}
        </button>
        {file && <span className="hint">{file.name}</span>}
      </div>
      {error && <p className="error">{error}</p>}

      {report && (
        <div className="report" role="status">
          <p className={report.status === 'refused' ? 'error' : 'warn'}>
            {report.status === 'refused'
              ? '转换器报告了不可放行的问题，已拒绝入库'
              : '转换产生了局部损失，需要你确认后才会入库'}
          </p>
          <ul>
            {report.blocking.map((w) => (
              <li key={w.digest} className="bad">
                {w.warning} — {w.reason}
              </li>
            ))}
            {report.losses.map((w) => (
              <li key={w.digest} className="loss">
                {w.warning}
              </li>
            ))}
          </ul>
          {report.notes.map((n) => (
            <p key={n} className="hint">
              {n}
            </p>
          ))}
          {report.hint && <p className="hint">{report.hint}</p>}
          {report.status !== 'refused' && (
            <button type="button" disabled={working} onClick={() => void send('all')}>
              已知悉损失，确认入库
            </button>
          )}
        </div>
      )}
    </section>
  );
}
