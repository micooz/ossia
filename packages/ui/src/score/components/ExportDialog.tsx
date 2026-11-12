import { zipSync } from 'fflate';
import { Eye, EyeOff, FileText, Image, Music2, Shapes, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { Button } from '../../components/ui/button.tsx';
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
  const [pdfPassword, setPdfPassword] = useState('');
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [width, setWidth] = useState(1600);
  const [pngQuality, setPngQuality] = useState(90);
  const [pngTransparent, setPngTransparent] = useState(false);
  const [fileName, setFileName] = useState('');
  const [versionRender, setVersionRender] = useState<RenderReference | null>(null);
  const [previewUrls, setPreviewUrls] = useState<Record<number, string>>({});
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState('');
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState('');
  const versionId = viewingVersion?.versionId ?? null;
  const versionDocumentId = viewingVersion?.documentId ?? null;
  const versionSeq = viewingVersion?.seq ?? null;
  const previewRender = versionId ? versionRender : currentRender;
  const pageNumbers = previewRender
    ? [...previewRender.manifest.pages]
        .sort((left, right) => left.page - right.page)
        .map((p) => p.page)
    : [];
  const pageCount = pageNumbers.length;
  const previewKey = previewRender?.renderKey ?? '';
  const pageKey = pageNumbers.join(',');
  const validWidth = Number.isInteger(width) && width >= 1 && width <= 4096;
  const validQuality = Number.isInteger(pngQuality) && pngQuality >= 1 && pngQuality <= 100;
  const canExport =
    !!exportVersionId &&
    fileName.trim().length > 0 &&
    (format !== 'png' || (validWidth && validQuality)) &&
    (!['png', 'svg'].includes(format) || pageCount > 0);

  useEffect(() => {
    if (!open) {
      dialog.current?.close();
      return;
    }
    setFormat('pdf');
    setPdfPassword('');
    setPasswordVisible(false);
    setWidth(1600);
    setPngQuality(90);
    setPngTransparent(false);
    setFileName(defaultFileName(documentTitle, versionSeq));
    setExportError('');
    setExporting(false);
    if (!dialog.current?.open) dialog.current?.showModal();
  }, [open, documentTitle, versionSeq, t]);

  useEffect(() => {
    if (!open) return;
    setPreviewError('');
    if (!versionId || !versionDocumentId) {
      setVersionRender(null);
      setPreviewLoading(false);
      return;
    }
    let live = true;
    setVersionRender(null);
    setPreviewLoading(true);
    api
      .renderVersion(versionDocumentId, versionId)
      .then((job) => api.waitJob<RenderReference>(job))
      .then((render) => {
        if (live) setVersionRender(render);
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
  }, [api, open, versionDocumentId, versionId]);

  useEffect(() => {
    if (!open || !previewKey || !pageKey) return;
    let live = true;
    const urls: string[] = [];
    setPreviewUrls({});
    setPreviewLoading(true);
    setPreviewError('');
    const targets = pageKey.split(',').map(Number);
    Promise.all(targets.map(async (page) => [page, await api.page(previewKey, page)] as const))
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
  }, [api, open, pageKey, previewKey]);

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
      const pageTargets = ['png', 'svg'].includes(format) ? pageNumbers : [null];
      const files: { page: number | null; blob: Blob }[] = [];
      for (const page of pageTargets) {
        const job = await api.output({
          source: { kind: 'version', versionId: exportVersionId },
          format,
          page,
          width: format === 'png' ? width : null,
          pdfPassword: format === 'pdf' && pdfPassword ? pdfPassword : null,
          pngQuality: format === 'png' ? pngQuality : null,
          pngTransparent: format === 'png' ? pngTransparent : null,
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
          {format === 'pdf' && (
            <div className="export-field">
              <label htmlFor="export-pdf-password">{t('pdfPassword')}</label>
              <div className="export-password">
                <input
                  id="export-pdf-password"
                  type={passwordVisible ? 'text' : 'password'}
                  autoComplete="new-password"
                  value={pdfPassword}
                  placeholder={t('optional')}
                  onChange={(event) => setPdfPassword(event.target.value)}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="export-password-toggle"
                  aria-label={passwordVisible ? t('hidePassword') : t('showPassword')}
                  aria-pressed={passwordVisible}
                  onClick={() => setPasswordVisible((visible) => !visible)}
                >
                  {passwordVisible ? (
                    <EyeOff aria-hidden="true" size={16} />
                  ) : (
                    <Eye aria-hidden="true" size={16} />
                  )}
                </Button>
              </div>
            </div>
          )}
          {format === 'png' && (
            <>
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
              <label className="export-field" htmlFor="export-quality">
                <span>
                  {t('pngQuality')} · {pngQuality}
                </span>
                <input
                  id="export-quality"
                  type="range"
                  min={1}
                  max={100}
                  step={1}
                  value={pngQuality}
                  onChange={(event) => setPngQuality(Number(event.target.value))}
                />
              </label>
              <label className="export-check" htmlFor="export-transparent">
                <input
                  id="export-transparent"
                  type="checkbox"
                  checked={pngTransparent}
                  onChange={(event) => setPngTransparent(event.target.checked)}
                />
                <span>{t('pngTransparent')}</span>
              </label>
            </>
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
                  {pageNumbers.map((page) =>
                    previewUrls[page] ? (
                      <img
                        key={page}
                        src={previewUrls[page]}
                        alt={t('scoreAlt', { page })}
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
