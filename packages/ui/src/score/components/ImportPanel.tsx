import { FileUp } from 'lucide-react';
import { useState } from 'react';

import { describe } from '../../errors.ts';
import type { ImportOutcome, ImportReport, InputFormat } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { ApiError } from '../api.ts';
import type { Api } from '../api.ts';

export function ImportPanel({
  api,
  onImported,
  onError,
}: {
  api: Api;
  onImported: (id: string) => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [file, setFile] = useState<File | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState('');
  async function send(confirmation: string | null = null) {
    if (!file) return;
    const extension = file.name.split('.').pop()?.toLowerCase();
    const format: InputFormat | undefined =
      extension === 'ly'
        ? 'ly'
        : ['xml', 'musicxml'].includes(extension ?? '')
          ? 'music-xml'
          : ['mid', 'midi'].includes(extension ?? '')
            ? 'midi'
            : undefined;
    if (!format) {
      setError(t('unsupportedFile'));
      return;
    }
    setWorking(true);
    setError('');
    try {
      const job = await api.import(file, {
        title: file.name.replace(/\.[^.]+$/, ''),
        format,
        confirmation,
        author: null,
        expect: null,
        waitMs: null,
      });
      const result = await api.waitJob<ImportOutcome>(job);
      setReport(result.report);
      if (result.document) {
        setFile(null);
        setReport(null);
        onImported(result.document.documentId);
      }
    } catch (e) {
      if (
        e instanceof ApiError &&
        e.detail &&
        ['import-confirm-required', 'import-refused'].includes(e.code)
      ) {
        const detail = e.detail as ImportReport | { report: ImportReport };
        setReport('report' in detail ? detail.report : detail);
      } else {
        setError(describe(e));
        onError(e);
      }
    } finally {
      setWorking(false);
    }
  }
  return (
    <section className="panel import">
      <h2>
        <FileUp aria-hidden="true" size={17} /> {t('importTitle')}
      </h2>
      <label htmlFor="score-file">{t('importFile')}</label>
      <input
        id="score-file"
        type="file"
        accept=".ly,.xml,.musicxml,.mid,.midi"
        disabled={working}
        onChange={(e) => {
          setFile(e.target.files?.[0] ?? null);
          setReport(null);
          setError('');
        }}
      />
      <button type="button" disabled={!file || working} onClick={() => void send()}>
        <FileUp aria-hidden="true" size={15} />
        {working ? t('converting') : t('importAction')}
      </button>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {report && (
        <div className="import-report" role="status">
          <p className={report.status === 'refused' ? 'error' : 'warn'}>
            {report.status === 'refused' ? t('importRefused') : t('importLosses')}
          </p>
          <ul>
            {[...report.blocking, ...report.losses].map((line, i) => (
              <li key={i}>{line}</li>
            ))}
          </ul>
          {report.diagnostics.map((d, i) => (
            <p key={i}>{d.message}</p>
          ))}
          {report.status === 'needs-confirmation' && report.confirmation && (
            <button type="button" disabled={working} onClick={() => void send(report.confirmation)}>
              {t('confirmImport')}
            </button>
          )}
        </div>
      )}
    </section>
  );
}
