import { useState } from 'react';

import { describe } from '../api.ts';
import type { Api } from '../api.ts';
import type { ExportFormat, ExportOutcome, Version } from '../generated/protocol.ts';
export function ExportBar({
  api,
  documentId,
  versions,
  onError,
}: {
  api: Api;
  documentId: string;
  versions: Version[];
  onError: (e: unknown) => void;
}) {
  const [version, setVersion] = useState(versions[0]?.versionId ?? '');
  const [format, setFormat] = useState<ExportFormat>('pdf');
  const [working, setWorking] = useState(false);
  const [result, setResult] = useState<{ jobId: string; outcome: ExportOutcome } | null>(null);
  const [error, setError] = useState('');
  async function run() {
    setWorking(true);
    setError('');
    setResult(null);
    try {
      const job = await api.export({ documentId, versionId: version, format, outputPath: null });
      const outcome = await api.waitJob<ExportOutcome>(job);
      setResult({ jobId: job.jobId, outcome });
    } catch (e) {
      setError(describe(e));
      onError(e);
    } finally {
      setWorking(false);
    }
  }
  return (
    <section className="panel export">
      <h2>导出已提交版本</h2>
      <div className="row">
        <label>
          版本{' '}
          <select
            aria-label="导出版本"
            value={version}
            disabled={working}
            onChange={(e) => setVersion(e.target.value)}
          >
            {versions.map((v) => (
              <option key={v.versionId} value={v.versionId}>
                版本 {v.seq}
              </option>
            ))}
          </select>
        </label>
        <label>
          格式{' '}
          <select
            aria-label="导出格式"
            value={format}
            disabled={working}
            onChange={(e) => setFormat(e.target.value as ExportFormat)}
          >
            <option value="pdf">PDF</option>
            <option value="midi">MIDI</option>
            <option value="ly">LilyPond 源码</option>
          </select>
        </label>
        <button type="button" disabled={working || !version} onClick={() => void run()}>
          {working ? '导出中…' : '导出'}
        </button>
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {result && (
        <div role="status">
          <p>
            已保存到 <code className="path">{result.outcome.path}</code>
          </p>
          <button
            type="button"
            onClick={() =>
              void api.reveal(result.jobId).catch((e) => {
                setError(describe(e));
                onError(e);
              })
            }
          >
            打开位置
          </button>
        </div>
      )}
    </section>
  );
}
