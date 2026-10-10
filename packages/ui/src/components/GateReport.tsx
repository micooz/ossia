// The five gates (ADR-0002 §6) are the product's safety story, so they are shown, not logged.
// A rejection without this panel is indistinguishable from a model failure in the user's eyes.
import type { GateReport as Gates } from '../api.ts';

const LABELS: Record<string, string> = {
  'G-a': '行边界（只改了授权行）',
  'G-b': '同行动邻近内容未被吞掉',
  'G-c': '编译通过（MIDI 通道）',
  'G-d': '爆炸半径 ≤ 所选小节的音符数',
  'G-e': '确实产生了音乐上的改变',
};

export function GateReport({ gate }: { gate: Gates }) {
  return (
    <ul className="gates">
      {(Object.keys(LABELS) as (keyof Gates)[]).map((k) => (
        <li key={k} className={gate[k]?.ok ? 'ok' : 'bad'}>
          <b>{k}</b> {LABELS[k as string]}
          {gate[k]?.detail && <span className="detail"> · {gate[k]!.detail}</span>}
        </li>
      ))}
    </ul>
  );
}
