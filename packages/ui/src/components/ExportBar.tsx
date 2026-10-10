// Service E in the MVP: PDF and MIDI of the version being viewed. Both come from the same render as
// the on-screen pages, so exporting costs nothing after a view (the cache is keyed by content hash).
import type { Api, VersionPayload } from '../api.ts';

interface Props {
  api: Api;
  docId: string;
  title: string;
  version: VersionPayload;
}

export function ExportBar({ api, docId, title, version }: Props) {
  const stamp = version.hash.slice(0, 8);
  const safe = (title || 'score').replace(/[^\w.-]+/g, '_');
  async function save(format: 'pdf' | 'midi') {
    await api.download(
      `/api/v1/documents/${docId}/export/${format}?hash=${encodeURIComponent(version.hash)}`,
      `${safe}-${stamp}.${format === 'pdf' ? 'pdf' : 'midi'}`,
    );
  }
  return (
    <div className="exports">
      <button
        type="button"
        disabled={!version.pdfUrl}
        title="导出 PDF"
        onClick={() => void save('pdf')}
      >
        PDF
      </button>
      <button
        type="button"
        disabled={!version.midiUrl}
        title="导出 MIDI（回放用）"
        onClick={() => void save('midi')}
      >
        MIDI
      </button>
    </div>
  );
}
