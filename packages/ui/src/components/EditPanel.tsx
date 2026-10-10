// Chain 3 in the UI: natural language in, gates + before/after out. The instruction is sent with
// the locators the player built by clicking, never with a source range (G1), and the result is a
// *candidate*: nothing moves the document head until accept (D10).
import { useState } from 'react';

import type { Api, Locator, VersionPayload } from '../api.ts';

interface Props {
  api: Api;
  docId: string;
  version: VersionPayload;
  locators: Locator[];
  busy: boolean;
  onResult: (jobId: string) => void;
  onBusy: (b: boolean) => void;
}

const PLACEHOLDER = '例如：把第 9 小节第一个音提高八度；或把这一小节的力度改成弱';

export function EditPanel({ api, docId, version, locators, busy, onResult, onBusy }: Props) {
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    if (!text.trim() || !locators.length) return;
    setError(null);
    onBusy(true);
    try {
      const { jobId } = await api.edit(docId, {
        baseHash: version.hash,
        locators,
        instruction: text.trim(),
      });
      setText('');
      onResult(jobId);
    } catch (e) {
      setError(String((e as Error).message));
      onBusy(false);
    }
  }

  return (
    <section className="panel edit">
      <h3>用自然语言修改</h3>
      <p className="hint">
        {locators.length
          ? `已选择 ${locators.length} 个小节：` +
            locators.map((l) => `谱表${(l.staff ?? 0) + 1} 第${l.measureStart}小节`).join('、')
          : '先在谱面上点击一个小节（可多选）。'}
      </p>
      <textarea
        value={text}
        rows={3}
        placeholder={PLACEHOLDER}
        aria-label="修改指令"
        onChange={(ev) => setText(ev.target.value)}
        onKeyDown={(ev) => {
          if ((ev.metaKey || ev.ctrlKey) && ev.key === 'Enter') void submit();
        }}
      />
      <div className="row">
        <button
          type="button"
          disabled={busy || !text.trim() || !locators.length}
          onClick={() => void submit()}
        >
          {busy ? '处理中…' : '提交修改'}
        </button>
        <span className="hint">⌘/Ctrl + Enter</span>
      </div>
      {error && <p className="error">{error}</p>}
    </section>
  );
}
