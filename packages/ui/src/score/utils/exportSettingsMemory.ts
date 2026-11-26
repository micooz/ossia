import type { OutputFormat } from '../../generated/protocol.ts';

export type ExportFormat = Exclude<OutputFormat, 'ly'>;

export interface ExportSettings {
  format: ExportFormat;
  fileName: string | null;
  includeDrawing: boolean;
  width: number;
  pngQuality: number;
  pngTransparent: boolean;
  pageRangeEnabled: boolean;
  pageStart: number;
  pageEnd: number;
  measureRangeEnabled: boolean;
  measureStart: number;
  measureEnd: number;
}

const defaults: ExportSettings = {
  format: 'pdf',
  fileName: null,
  includeDrawing: true,
  width: 1600,
  pngQuality: 90,
  pngTransparent: false,
  pageRangeEnabled: false,
  pageStart: 1,
  pageEnd: 1,
  measureRangeEnabled: false,
  measureStart: 1,
  measureEnd: 1,
};

const formats: ExportFormat[] = ['pdf', 'png', 'svg', 'midi', 'wav'];
const storageKey = (documentId: string) => `ossia:export-settings:v1:${documentId}`;

/** Reads optional export preferences stored for one score. */
export function readExportSettings(documentId: string): ExportSettings {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(storageKey(documentId)) ?? 'null');
    if (!value || typeof value !== 'object' || Array.isArray(value)) return { ...defaults };
    const stored = value as Record<string, unknown>;
    const result = { ...defaults };
    if (typeof stored.format === 'string' && formats.includes(stored.format as ExportFormat)) {
      result.format = stored.format as ExportFormat;
    }
    if (typeof stored.fileName === 'string') result.fileName = stored.fileName;
    for (const key of [
      'includeDrawing',
      'pngTransparent',
      'pageRangeEnabled',
      'measureRangeEnabled',
    ] as const) {
      if (typeof stored[key] === 'boolean') result[key] = stored[key];
    }
    for (const key of [
      'width',
      'pngQuality',
      'pageStart',
      'pageEnd',
      'measureStart',
      'measureEnd',
    ] as const) {
      if (typeof stored[key] === 'number' && Number.isFinite(stored[key]))
        result[key] = stored[key];
    }
    return result;
  } catch {
    return { ...defaults };
  }
}

/** Saves export preferences without interrupting the score page if storage is unavailable. */
export function rememberExportSettings(documentId: string, settings: ExportSettings) {
  try {
    localStorage.setItem(storageKey(documentId), JSON.stringify(settings));
  } catch {
    // Export settings are optional browser state.
  }
}
