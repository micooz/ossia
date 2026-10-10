// Chain 3's decision surface. D10 requires the player to judge the *rendered* result, so before and
// after are shown side by side as real pages, with the gate report and the model's own one-line why
// underneath. Accept is a CAS commit on the server; a moved head is refused (409), never merged.
import { useEffect, useState } from 'react';

import type { Api, EditResultView } from '../api.ts';
import { ApiError } from '../api.ts';
import { GateReport } from './GateReport.tsx';

interface Props {
  api: Api;
  docId: string;
  result: EditResultView;
  /** Called after accept/discard with the head the server moved to (accept) or the unchanged
   *  head (discard), so the app can view the result instead of the pre-decision snapshot. */
  onDecided: (headHash: string) => void;
  onBusy: (b: boolean) => void;
}

export function CompareView({ api, docId, result, onDecided, onBusy }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setBusy(false);
    setError(null);
  }, [result.patchId]);

  async function act(action: 'accept' | 'discard') {
    setBusy(true);
    onBusy(true);
    setError(null);
    try {
      const r = await api.patchAction(docId, result.patchId, action);
      // `headHash` is present on accept only; a discard leaves the chain where it was.
      onDecided(r.headHash ?? result.baseHash);
    } catch (e) {
      // A conflict means the head moved while this candidate was open: the candidate is kept, so
      // the player can re-select and retry rather than losing the work.
      setError(
        e instanceof ApiError && e.code === 'conflict'
          ? '文档已被修改，此候选未能应用（仍保留在历史中）。请重新选择目标小节后再试。'
          : String((e as Error).message),
      );
      setBusy(false);
      onBusy(false);
    }
  }

  const rejected = result.status === 'rejected';
  return (
    <section className="panel compare">
      <h3>{rejected ? '未通过校验' : '修改前后对比'}</h3>
      <p className={rejected ? 'error' : 'ok'}>
        {result.why ?? (rejected ? '模型未能给出可应用的修改' : '五道校验全部通过')}
      </p>
      <GateReport gate={result.gate} />

      {result.attempts > 1 && (
        <p className="hint">经过 {result.attempts} 次尝试（含一次反馈重试）。</p>
      )}
      {result.model.usage && (
        <p className="hint">
          模型 {result.model.name} · {Math.round(result.model.ms)} ms · prompt{' '}
          {result.model.usage.prompt} / completion {result.model.usage.completion} tokens
        </p>
      )}
      {result.tokens && (
        <p className="hint mono">
          tokens +[{result.tokens.added.join(' ')}] -[{result.tokens.removed.join(' ')}]
        </p>
      )}
      {result.targets.length > 0 && (
        <p className="hint mono">
          授权行：{result.targets.map((t) => `${t.tag} L${t.lineStart}-${t.lineEnd}`).join('，')}
        </p>
      )}

      {!rejected && result.before && result.after && (
        <div className="sbs">
          {[
            ['修改前', result.before],
            ['修改后', result.after],
          ].map(([label, p]) => (
            <figure key={label as string}>
              <figcaption>
                {label as string} · {typeof p === 'object' ? p.renderMs : 0} ms
              </figcaption>
              {typeof p === 'object' && <Page api={api} url={p.svgUrl} />}
            </figure>
          ))}
        </div>
      )}

      {rejected ? (
        <button type="button" onClick={() => onDecided(result.baseHash)}>
          知道了
        </button>
      ) : (
        <div className="row">
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void act('accept')}
          >
            接受
          </button>
          <button type="button" disabled={busy} onClick={() => void act('discard')}>
            放弃
          </button>
        </div>
      )}
      {error && <p className="error">{error}</p>}
    </section>
  );
}

/** One page of one version, fetched through the client so the token header is attached. */
function Page({ api, url }: { api: Api; url: string }) {
  const [svg, setSvg] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setSvg(null);
    api
      .fetchText(url)
      .then((t) => {
        if (live) setSvg(t);
      })
      .catch(() => {
        if (live) setSvg(null);
      });
    return () => {
      live = false;
    };
  }, [api, url]);
  if (!svg) return <div className="thumb loading">载入中…</div>;
  return <div className="thumb" dangerouslySetInnerHTML={{ __html: svg }} />;
}
