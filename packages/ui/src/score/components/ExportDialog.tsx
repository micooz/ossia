import { zipSync } from 'fflate';
import {
  AlertCircle,
  AudioLines,
  Eye,
  EyeOff,
  FileText,
  Image,
  Music2,
  Shapes,
  X,
} from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';

import { Button } from '../../components/ui/button.tsx';
import { Checkbox } from '../../components/ui/checkbox.tsx';
import { Input } from '../../components/ui/input.tsx';
import { Slider } from '../../components/ui/slider.tsx';
import { describe } from '../../errors.ts';
import type { OutputBatchOutcome, RenderReference, Version } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { DrawingController } from '../hooks/useDrawing.ts';
import type { Api } from '../utils/api.ts';
import { readExportSettings, rememberExportSettings } from '../utils/exportSettingsMemory.ts';
import type { ExportFormat, ExportSettings } from '../utils/exportSettingsMemory.ts';
import { loadScorePlayback } from '../utils/renderCache.ts';
import { DrawingLayer } from './DrawingLayer.tsx';

const PNG_QUALITY_MIN = 30;
const PNG_QUALITY_MAX = 100;

const FORMATS = [
  { value: 'pdf', label: 'PDF', icon: FileText, description: 'pdfDescription' },
  { value: 'png', label: 'PNG', icon: Image, description: 'pngDescription' },
  { value: 'svg', label: 'SVG', icon: Shapes, description: 'svgDescription' },
  { value: 'midi', label: 'MIDI', icon: Music2, description: 'midiDescription' },
  { value: 'wav', label: 'WAV', icon: AudioLines, description: 'wavDescription' },
] as const;

const FORMAT_EXTENSIONS: Record<ExportFormat, string> = {
  pdf: 'pdf',
  midi: 'mid',
  png: 'png',
  svg: 'svg',
  wav: 'wav',
};

function defaultFileName(title: string, version: number | null) {
  const base = title.trim() || 'score';
  return version ? `${base} - v${version}` : base;
}

function triggerBlobDownload(blob: Blob, fileName: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1_000);
}

function triggerUrlDownload(url: string, fileName: string) {
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  link.rel = 'noopener';
  document.body.append(link);
  link.click();
  link.remove();
}

export function ExportDialog({
  api,
  open,
  onClose,
  documentTitle,
  viewingVersion,
  exportVersionId,
  currentRender,
  drawing,
  documentId,
}: {
  api: Api;
  open: boolean;
  onClose: () => void;
  documentTitle: string;
  viewingVersion: Version | null;
  exportVersionId: string | null;
  currentRender: RenderReference | null;
  drawing: DrawingController;
  documentId: string;
}) {
  const { t } = useI18n();
  const dialog = useRef<HTMLDialogElement>(null);
  const previewStage = useRef<HTMLDivElement>(null);
  const loadedSettings = useMemo(() => readExportSettings(documentId), [documentId]);
  const [memory, setMemory] = useState(() => ({ documentId, settings: loadedSettings }));
  const settings = memory.documentId === documentId ? memory.settings : loadedSettings;
  const {
    format,
    includeDrawing,
    width,
    pngQuality,
    pngTransparent,
    pageRangeEnabled,
    pageStart,
    pageEnd,
    measureRangeEnabled,
    measureStart,
    measureEnd,
  } = settings;
  const fileName = settings.fileName ?? defaultFileName(documentTitle, viewingVersion?.seq ?? null);
  function updateSettings(changes: Partial<ExportSettings>) {
    const next = { ...settings, ...changes };
    setMemory({ documentId, settings: next });
    rememberExportSettings(documentId, next);
  }
  const [pageAspects, setPageAspects] = useState<Record<number, number>>({});
  const [pdfPassword, setPdfPassword] = useState('');
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [versionRender, setVersionRender] = useState<RenderReference | null>(null);
  const [previewUrls, setPreviewUrls] = useState<Record<number, string>>({});
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState('');
  const [previewAttempt, setPreviewAttempt] = useState(0);
  const [visiblePage, setVisiblePage] = useState(1);
  const [previewScrollEdges, setPreviewScrollEdges] = useState({
    start: false,
    end: false,
  });
  const [maxMeasure, setMaxMeasure] = useState<number | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState('');
  const versionId = exportVersionId;
  const versionDocumentId = documentId;
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
  const validQuality =
    Number.isInteger(pngQuality) && pngQuality >= PNG_QUALITY_MIN && pngQuality <= PNG_QUALITY_MAX;
  const validMeasureRange =
    !measureRangeEnabled ||
    (maxMeasure !== null &&
      Number.isInteger(measureStart) &&
      Number.isInteger(measureEnd) &&
      measureStart >= 1 &&
      measureEnd >= measureStart &&
      measureEnd <= maxMeasure);
  const validPageRange =
    !pageRangeEnabled ||
    (Number.isInteger(pageStart) &&
      Number.isInteger(pageEnd) &&
      pageStart >= 1 &&
      pageEnd >= pageStart &&
      pageEnd <= pageCount);
  const canExport =
    !!exportVersionId &&
    fileName.trim().length > 0 &&
    (format !== 'png' || (validWidth && validQuality)) &&
    (!['png', 'svg'].includes(format) || pageCount > 0) &&
    (!(format === 'pdf' && pageRangeEnabled) || pageCount > 0) &&
    (!['pdf', 'png', 'svg'].includes(format) || validPageRange) &&
    (!['midi', 'wav'].includes(format) || validMeasureRange) &&
    (!includeDrawing ||
      !['pdf', 'png', 'svg'].includes(format) ||
      (drawing.ready && !drawing.error && !drawing.saving));

  useEffect(() => {
    if (!open) {
      dialog.current?.close();
      return;
    }
    setVisiblePage(1);
    setPdfPassword('');
    setPasswordVisible(false);
    setMaxMeasure(null);
    setExportError('');
    setExporting(false);
    if (!dialog.current?.open) dialog.current?.showModal();
  }, [open]);

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
  }, [api, open, versionDocumentId, versionId, previewAttempt]);

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

  useEffect(() => {
    const stage = previewStage.current;
    if (!open || !stage) return;
    const update = () => {
      const next = {
        start: stage.scrollTop > 1,
        end: stage.scrollTop + stage.clientHeight < stage.scrollHeight - 1,
      };
      const pages = Array.from(stage.querySelectorAll<HTMLElement>('[data-preview-page]'));
      const center = stage.getBoundingClientRect().top + stage.clientHeight / 2;
      const closest = pages.reduce<HTMLElement | null>((best, page) => {
        const distance = (node: HTMLElement) =>
          Math.abs(
            (node.getBoundingClientRect().top + node.getBoundingClientRect().bottom) / 2 - center,
          );
        return !best || distance(page) < distance(best) ? page : best;
      }, null);
      if (closest) setVisiblePage(Number(closest.dataset.previewPage));
      setPreviewScrollEdges((current) =>
        current.start === next.start && current.end === next.end ? current : next,
      );
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(stage);
    const content = stage.firstElementChild;
    if (content) observer.observe(content);
    stage.addEventListener('scroll', update, { passive: true });
    return () => {
      observer.disconnect();
      stage.removeEventListener('scroll', update);
    };
  }, [open, pageKey, previewError, previewKey, previewLoading]);

  useEffect(() => {
    if (!open || !['midi', 'wav'].includes(format) || !previewKey) {
      setMaxMeasure(null);
      return;
    }
    let live = true;
    setMaxMeasure(null);
    loadScorePlayback(api, previewKey)
      .then((playback) => {
        if (!live) return;
        const last = playback.systems.reduce(
          (maximum, system) => Math.max(maximum, system.measureEnd),
          0,
        );
        setMaxMeasure(last > 0 ? last : null);
      })
      .catch(() => {
        if (live) setMaxMeasure(null);
      });
    return () => {
      live = false;
    };
  }, [api, format, open, previewKey]);

  function chooseFormat(next: ExportFormat) {
    updateSettings({ format: next });
    setExportError('');
  }

  function toggleMeasureRange(enabled: boolean) {
    updateSettings(
      enabled && maxMeasure !== null
        ? { measureRangeEnabled: true, measureStart: 1, measureEnd: maxMeasure }
        : { measureRangeEnabled: enabled },
    );
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
      const visualPages =
        ['png', 'svg'].includes(format) || (format === 'pdf' && pageRangeEnabled)
          ? pageNumbers.filter(
              (page) => !pageRangeEnabled || (page >= pageStart && page <= pageEnd),
            )
          : null;
      const job = await api.output({
        source: { kind: 'version', versionId: exportVersionId },
        format,
        pages: visualPages,
        width: format === 'png' ? width : null,
        pdfPassword: format === 'pdf' && pdfPassword ? pdfPassword : null,
        pngQuality: format === 'png' ? pngQuality : null,
        pngTransparent: format === 'png' ? pngTransparent : null,
        includeDrawing: ['pdf', 'png', 'svg'].includes(format) && includeDrawing,
        outputPath: null,
        outputDirectory: null,
        crop: null,
        paddingStaffSpaces: null,
        audioRange:
          ['midi', 'wav'].includes(format) && measureRangeEnabled
            ? { start: measureStart, end: measureEnd }
            : null,
        waitMs: null,
      });
      const result = await api.waitJob<OutputBatchOutcome>(job);
      if (format === 'pdf' || format === 'wav') {
        const downloadName = `${safeBase}.${FORMAT_EXTENSIONS[format]}`;
        triggerUrlDownload(api.outputDownloadUrl(job.jobId, downloadName), downloadName);
        return;
      }
      const files =
        visualPages && visualPages.length > 1
          ? await Promise.all(
              result.files.map(async (file, index) => ({
                page: file.page,
                blob: await api.outputBatchFile(job.jobId, index),
              })),
            )
          : [{ page: visualPages?.[0] ?? null, blob: await api.outputFile(job.jobId) }];
      if (files.length === 1) {
        triggerBlobDownload(files[0]!.blob, `${safeBase}.${FORMAT_EXTENSIONS[format]}`);
      } else {
        const archive: Record<string, Uint8Array> = {};
        for (const file of files) {
          archive[`${safeBase}-${file.page}.${FORMAT_EXTENSIONS[format]}`] = new Uint8Array(
            await file.blob.arrayBuffer(),
          );
        }
        triggerBlobDownload(
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

  const pageControls = (
    <div className={`export-measure-range${!pageRangeEnabled ? ' export-page-range-compact' : ''}`}>
      <label className="export-check" htmlFor="export-page-range">
        <Checkbox
          id="export-page-range"
          checked={pageRangeEnabled}
          disabled={!pageCount}
          onCheckedChange={(checked) => {
            updateSettings(
              checked === true
                ? { pageRangeEnabled: true, pageStart: 1, pageEnd: pageCount }
                : { pageRangeEnabled: false },
            );
          }}
        />
        <span>{t('exportPageRange')}</span>
      </label>
      {pageRangeEnabled && (
        <div className="export-measure-fields">
          <label className="export-field" htmlFor="export-page-start">
            <span>{t('startPage')}</span>
            <Input
              id="export-page-start"
              type="number"
              min={1}
              max={pageCount}
              step={1}
              value={pageStart}
              onChange={(event) => updateSettings({ pageStart: Number(event.target.value) })}
            />
          </label>
          <label className="export-field" htmlFor="export-page-end">
            <span>{t('endPage')}</span>
            <Input
              id="export-page-end"
              type="number"
              min={pageStart}
              max={pageCount}
              step={1}
              value={pageEnd}
              onChange={(event) => updateSettings({ pageEnd: Number(event.target.value) })}
            />
          </label>
        </div>
      )}
    </div>
  );

  const drawingControls = (
    <div className="export-drawing">
      <label className="export-check" htmlFor="export-include-drawing">
        <Checkbox
          id="export-include-drawing"
          checked={includeDrawing}
          onCheckedChange={(checked) => updateSettings({ includeDrawing: checked === true })}
        />
        <span>{t('includeDrawing')}</span>
      </label>
      {includeDrawing && drawing.error && (
        <div className="export-error" role="alert">
          <AlertCircle aria-hidden="true" size={18} />
          <div>
            <p>{t('drawingSaveFailed')}</p>
            <button type="button" onClick={drawing.retry}>
              {t('retry')}
            </button>
          </div>
        </div>
      )}
    </div>
  );

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
        <h2 id="export-dialog-title" tabIndex={-1} autoFocus>
          {t('exportDialogTitle')}
        </h2>
        <ButtonClose disabled={exporting} onClose={onClose} />
      </div>
      <div className="export-dialog-body">
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
        <form
          id="export-settings-form"
          className="export-settings"
          aria-label={t('exportSettings')}
          onSubmit={(event) => {
            event.preventDefault();
            if (!exporting) void exportFile();
          }}
        >
          <label className="export-field" htmlFor="export-file-name">
            <span>{t('fileName')}</span>
            <Input
              id="export-file-name"
              value={fileName}
              onChange={(event) => updateSettings({ fileName: event.target.value })}
            />
          </label>
          {['midi', 'wav'].includes(format) && (
            <div className="export-measure-range">
              <label className="export-check" htmlFor="export-measure-range">
                <Checkbox
                  id="export-measure-range"
                  checked={measureRangeEnabled}
                  disabled={maxMeasure === null}
                  onCheckedChange={(checked) => toggleMeasureRange(checked === true)}
                />
                <span>{t('exportMeasureRange')}</span>
              </label>
              {maxMeasure === null && (
                <p className="hint export-measure-hint">{t('measureRangeUnavailable')}</p>
              )}
              {measureRangeEnabled && (
                <div className="export-measure-fields">
                  <label className="export-field" htmlFor="export-measure-start">
                    <span>{t('startMeasure')}</span>
                    <Input
                      id="export-measure-start"
                      type="number"
                      min={1}
                      max={maxMeasure ?? undefined}
                      step={1}
                      value={measureStart}
                      onChange={(event) =>
                        updateSettings({ measureStart: Number(event.target.value) })
                      }
                    />
                  </label>
                  <label className="export-field" htmlFor="export-measure-end">
                    <span>{t('endMeasure')}</span>
                    <Input
                      id="export-measure-end"
                      type="number"
                      min={measureStart}
                      max={maxMeasure ?? undefined}
                      step={1}
                      value={measureEnd}
                      onChange={(event) =>
                        updateSettings({ measureEnd: Number(event.target.value) })
                      }
                    />
                  </label>
                </div>
              )}
            </div>
          )}
          {format === 'pdf' && (
            <div className="export-field">
              <label htmlFor="export-pdf-password">{t('pdfPassword')}</label>
              <div className="export-password">
                <Input
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
          {format === 'pdf' && (
            <>
              {pageControls}
              {drawingControls}
            </>
          )}
          {format === 'png' && (
            <>
              <label className="export-field" htmlFor="export-width">
                <span>{t('pixelWidth')}</span>
                <Input
                  id="export-width"
                  type="number"
                  min={1}
                  max={4096}
                  step={1}
                  value={width}
                  onChange={(event) => updateSettings({ width: Number(event.target.value) })}
                />
              </label>
              <div className="export-field">
                <span>
                  {t('pngQuality')} {pngQuality}%
                </span>
                <div className="export-quality-slider">
                  <span className="export-quality-disabled" aria-hidden="true" />
                  <Slider
                    id="export-quality"
                    aria-label={t('pngQuality')}
                    min={PNG_QUALITY_MIN}
                    max={PNG_QUALITY_MAX}
                    step={1}
                    value={[pngQuality]}
                    onValueChange={([value]) =>
                      updateSettings({ pngQuality: value ?? PNG_QUALITY_MIN })
                    }
                  />
                </div>
              </div>
              {pageControls}
              {drawingControls}
              <label className="export-check" htmlFor="export-transparent">
                <Checkbox
                  id="export-transparent"
                  checked={pngTransparent}
                  onCheckedChange={(checked) =>
                    updateSettings({ pngTransparent: checked === true })
                  }
                />
                <span>{t('pngTransparent')}</span>
              </label>
            </>
          )}
          {format === 'svg' && (
            <>
              {pageControls}
              {drawingControls}
            </>
          )}
          {exportError && (
            <p className="error" role="alert">
              {t('exportFailed', { error: exportError })}
            </p>
          )}
        </form>
        <section className="export-preview" aria-label={t('exportPreview')}>
          <div
            ref={previewStage}
            className={[
              'export-preview-stage',
              previewScrollEdges.start ? 'has-more-start' : '',
              previewScrollEdges.end ? 'has-more-end' : '',
            ]
              .filter(Boolean)
              .join(' ')}
          >
            {previewLoading ? (
              <p className="score-loading">{t('loadingScore')}</p>
            ) : previewError ? (
              <div className="export-preview-error" role="alert">
                <AlertCircle aria-hidden="true" size={24} />
                <p>{t('scoreLoadFailed', { error: previewError })}</p>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setPreviewAttempt((value) => value + 1)}
                >
                  {t('retry')}
                </Button>
              </div>
            ) : Object.keys(previewUrls).length ? (
              <div className="export-preview-pages">
                {pageNumbers.map((page) =>
                  previewUrls[page] ? (
                    <div className="export-preview-page" key={page} data-preview-page={page}>
                      <img
                        src={previewUrls[page]}
                        alt={t('scoreAlt', { page })}
                        draggable={false}
                        onLoad={(event) => {
                          const image = event.currentTarget;
                          setPageAspects((current) => ({
                            ...current,
                            [page]: image.naturalHeight / image.naturalWidth,
                          }));
                        }}
                      />
                      {includeDrawing &&
                        ['pdf', 'png', 'svg'].includes(format) &&
                        pageAspects[page] && (
                          <DrawingLayer
                            page={page}
                            aspect={pageAspects[page]}
                            strokes={drawing.drawing.strokes}
                          />
                        )}
                    </div>
                  ) : null,
                )}
              </div>
            ) : (
              <p className="hint">{t('noRenderTitle')}</p>
            )}
          </div>
          {pageCount > 0 &&
            !previewLoading &&
            !previewError &&
            Object.keys(previewUrls).length > 0 && (
              <span className="export-page-count">
                {t('currentPageCount', { page: visiblePage, count: pageCount })}
              </span>
            )}
        </section>
      </div>
      <div className="export-dialog-footer">
        <button
          type="submit"
          form="export-settings-form"
          className="export-submit"
          disabled={exporting || !canExport}
          aria-busy={exporting}
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
