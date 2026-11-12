import { zipSync } from 'fflate';
import { FileText, Image, Music2, Shapes, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';

import { describe } from '../../errors.ts';
import type { OutputFormat, RenderReference, Version } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { Api } from '../api.ts';

type ExportFormat = Exclude<OutputFormat, 'ly'>;

const FORMATS = [
  { value: 'pdf', label: 'PDF', icon: FileText, description: 'pdfDescription' },
  { value: 'png', label: 'PNG', icon: Image, description: 'pngDescription' },
  { value: 'svg', label: 'SVG', icon: Shapes, description: 'svgDescription' },
  { value: 'midi', label: 'MIDI', icon: Music2, description: 'midiDescription' },
] as const;

const FORMAT_EXTENSIONS: Record<ExportFormat, string> = {
  pdf: 'pdf',
  midi: 'mid',
  png: 'png',
  svg: 'svg',
};

function defaultFileName(title: string, version: number | null) {
  const base = title.trim() || 'score';
  return version ? `${base} - v${version}` : base;
}

function triggerDownload(blob: Blob, fileName: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1_000);
}

export function ExportDialog({
  api,
  open,
  onClose,
  documentTitle,
  viewingVersion,
  exportVersionId,
  currentRender,
}: {
  api: Api;
  open: boolean;
  onClose: () => void;
  documentTitle: string;
  viewingVersion: Version | null;
  exportVersionId: string | null;
  currentRender: RenderReference | null;
}) {
  const { t } = useI18n();
  const dialog = useRef<HTMLDialogElement>(null);
  const [format, setFormat] = useState<ExportFormat>('pdf');
  const [width, setWidth] = useState(1600);
  const [fileName, setFileName] = useState('');
  const [previewRender, setPreviewRender] = useState<RenderReference | null>(null);
  const [previewUrls, setPreviewUrls] = useState<Record<number, string>>({});
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState('');
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState('');
  const versionId = viewingVersion?.versionId ?? null;
  const versionDocumentId = viewingVersion?.documentId ?? null;
  const versionSeq = viewingVersion?.seq ?? null;
  const pages = useMemo(
    () =>
      previewRender
        ? [...previewRender.manifest.pages].sort((left, right) => left.page - right.page)
        : [],
    [previewRender],
  );
  const pageCount = pages.length;
  const validWidth = Number.isInteger(width) && width >= 1 && width <= 4096;
  const canExport =
    !!exportVersionId &&
    fileName.trim().length > 0 &&
    (format !== 'png' || validWidth) &&
    (!['png', 'svg'].includes(format) || pageCount > 0);

  useEffect(() => {
    if (!open) {
      dialog.current?.close();
      return;
    }
    setFormat('pdf');
    setWidth(1600);
    setFileName(defaultFileName(documentTitle, versionSeq));
    setExportError('');
    setExporting(false);
    if (!dialog.current?.open) dialog.current?.showModal();
  }, [open, documentTitle, versionSeq, t]);

  useEffect(() => {
    if (!open) return;
    setPreviewError('');
    if (!versionId || !versionDocumentId) {
      setPreviewRender(currentRender);
      setPreviewLoading(false);
      return;
    }
    let live = true;
    setPreviewRender(null);
    setPreviewLoading(true);
    api
      .renderVersion(versionDocumentId, versionId)
      .then((job) => api.waitJob<RenderReference>(job))
      .then((render) => {
        if (live) setPreviewRender(render);
      })
      .catch((reason) => {
        if (live) setPreviewError(describe(reason));
      })
      .finally(() => {
        if (live) setPreviewLoading(false);
      });
    return () => {
      live = false;
    };
  }, [api, currentRender, open, versionDocumentId, versionId]);

  useEffect(() => {
    if (!open || format === 'midi' || !previewRender || !pages.length) {
      setPreviewUrls({});
      return;
    }
    let live = true;
    const urls: string[] = [];
    setPreviewLoading(true);
    setPreviewError('');
    Promise.all(
      pages.map(
        async (page) => [page.page, await api.page(previewRender.renderKey, page.page)] as const,
      ),
    )
      .then((loaded) => {
        if (!live) return;
        const next: Record<number, string> = {};
        for (const [page, blob] of loaded) {
          const url = URL.createObjectURL(blob);
          urls.push(url);
          next[page] = url;
        }
        setPreviewUrls(next);
      })
      .catch((reason) => {
        if (live) setPreviewError(describe(reason));
      })
      .finally(() => {
        if (live) setPreviewLoading(false);
      });
    return () => {
      live = false;
      urls.forEach((url) => URL.revokeObjectURL(url));
    };
  }, [api, format, open, pages, previewRender]);

  function chooseFormat(next: ExportFormat) {
    setFormat(next);
    setExportError('');
  }

  async function exportFile() {
    if (!exportVersionId || !canExport) return;
    setExporting(true);
    setExportError('');
    try {
      const base = fileName
        .trim()
        .replace(/[\\/:*?"<>|]+/g, '-')
        .replace(/[. ]+$/g, '');
      const safeBase = base || 'score';
      const pageTargets = ['png', 'svg'].includes(format)
        ? pages.map((candidate) => candidate.page)
        : [null];
      const files: { page: number | null; blob: Blob }[] = [];
      for (const page of pageTargets) {
        const job = await api.output({
          source: { kind: 'version', versionId: exportVersionId },
          format,
          page,
          width: format === 'png' ? width : null,
          outputPath: null,
          waitMs: null,
        });
        await api.waitJob(job);
        files.push({ page, blob: await api.outputFile(job.jobId) });
      }
      if (files.length === 1) {
        triggerDownload(files[0]!.blob, `${safeBase}.${FORMAT_EXTENSIONS[format]}`);
      } else {
        const archive: Record<string, Uint8Array> = {};
        for (const file of files) {
          archive[`${safeBase}-${file.page}.${FORMAT_EXTENSIONS[format]}`] = new Uint8Array(
            await file.blob.arrayBuffer(),
          );
        }
        triggerDownload(
          new Blob([zipSync(archive, { level: 0 })], { type: 'application/zip' }),
          `${safeBase}.zip`,
        );
      }
    } catch (error) {
      setExportError(describe(error));
    } finally {
      setExporting(false);
    }
  }

  return (
    <dialog
      ref={dialog}
      className="export-dialog"
      aria-labelledby="export-dialog-title"
      onCancel={(event) => {
        if (exporting) {
          event.preventDefault();
          return;
        }
        event.preventDefault();
        onClose();
      }}
    >
      <div className="export-dialog-header">
        <h2 id="export-dialog-title">{t('exportDialogTitle')}</h2>
        <ButtonClose disabled={exporting} onClose={onClose} />
      </div>
      <div className={`export-dialog-body ${format === 'midi' ? 'no-preview' : ''}`}>
        <nav className="export-formats" aria-label={t('exportFormatHeading')}>
          {FORMATS.map((option) => {
            const Icon = option.icon;
            return (
              <button
                key={option.value}
                type="button"
                className="export-format"
                aria-pressed={format === option.value}
                onClick={() => chooseFormat(option.value)}
              >
                <Icon aria-hidden="true" size={19} />
                <span>
                  <strong>{option.label}</strong>
                  <small>{t(option.description)}</small>
                </span>
              </button>
            );
          })}
        </nav>
        <section className="export-settings" aria-label={t('exportSettings')}>
          <label className="export-field" htmlFor="export-file-name">
            <span>{t('fileName')}</span>
            <input
              id="export-file-name"
              value={fileName}
              onChange={(event) => setFileName(event.target.value)}
            />
          </label>
          {format === 'png' && (
            <label className="export-field" htmlFor="export-width">
              <span>{t('pixelWidth')}</span>
              <input
                id="export-width"
                type="number"
                min={1}
                max={4096}
                step={1}
                value={width}
                onChange={(event) => setWidth(Number(event.target.value))}
              />
            </label>
          )}
          {exportError && (
            <p className="error" role="alert">
              {t('exportFailed', { error: exportError })}
            </p>
          )}
        </section>
        {format !== 'midi' && (
          <section className="export-preview" aria-label={t('exportPreview')}>
            <div className="export-preview-stage">
              {previewLoading ? (
                <p className="score-loading">{t('loadingScore')}</p>
              ) : previewError ? (
                <p className="error" role="alert">
                  {t('scoreLoadFailed', { error: previewError })}
                </p>
              ) : Object.keys(previewUrls).length ? (
                <div className="export-preview-pages">
                  {pages.map((page) =>
                    previewUrls[page.page] ? (
                      <img
                        key={page.page}
                        src={previewUrls[page.page]}
                        alt={t('scoreAlt', { page: page.page })}
                        draggable={false}
                      />
                    ) : null,
                  )}
                </div>
              ) : (
                <p className="hint">{t('noRenderTitle')}</p>
              )}
            </div>
            {pageCount > 0 && (
              <span className="export-page-count">{t('totalPages', { count: pageCount })}</span>
            )}
          </section>
        )}
      </div>
      <div className="export-dialog-footer">
        <button
          type="button"
          className="export-submit"
          disabled={exporting || !canExport}
          aria-busy={exporting}
          onClick={() => void exportFile()}
        >
          {exporting ? t('exporting') : t('exportAction')}
        </button>
      </div>
    </dialog>
  );
}

function ButtonClose({ disabled, onClose }: { disabled: boolean; onClose: () => void }) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      className="export-dialog-close"
      aria-label={t('close')}
      disabled={disabled}
      onClick={onClose}
    >
      <X aria-hidden="true" size={18} />
    </button>
  );
}
