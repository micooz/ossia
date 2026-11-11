import { Download, ExternalLink } from 'lucide-react';
import { useState } from 'react';

import { describe } from '../../errors.ts';
import type { OutputFormat, OutputOutcome, Version } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { Api } from '../api.ts';
export function ExportBar({
  api,
  versions,
  onError,
}: {
  api: Api;
  versions: Version[];
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [version, setVersion] = useState(versions[0]?.versionId ?? '');
  const [format, setFormat] = useState<OutputFormat>('pdf');
  const [working, setWorking] = useState(false);
  const [result, setResult] = useState<{ jobId: string; outcome: OutputOutcome } | null>(null);
  const [error, setError] = useState('');
  async function run() {
    setWorking(true);
    setError('');
    setResult(null);
    try {
      const job = await api.output({
        source: { kind: 'version', versionId: version },
        format,
        page: null,
        width: null,
        outputPath: null,
        waitMs: null,
      });
      const outcome = await api.waitJob<OutputOutcome>(job);
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
      <h2>{t('exportTitle')}</h2>
      <div className="row">
        <label>
          {t('exportVersion')}{' '}
          <select
            aria-label={t('exportVersionLabel')}
            value={version}
            disabled={working}
            onChange={(e) => setVersion(e.target.value)}
          >
            {versions.map((v) => (
              <option key={v.versionId} value={v.versionId}>
                {t('version')} {v.seq}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t('format')}{' '}
          <select
            aria-label={t('exportFormatLabel')}
            value={format}
            disabled={working}
            onChange={(e) => setFormat(e.target.value as OutputFormat)}
          >
            <option value="pdf">PDF</option>
            <option value="midi">MIDI</option>
            <option value="ly">{t('lilypondSource')}</option>
          </select>
        </label>
        <button type="button" disabled={working || !version} onClick={() => void run()}>
          <Download aria-hidden="true" size={15} />
          {working ? t('exporting') : t('exportAction')}
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
            {t('savedTo')} <code className="path">{result.outcome.path}</code>
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
            <ExternalLink aria-hidden="true" size={15} />
            {t('reveal')}
          </button>
        </div>
      )}
    </section>
  );
}
