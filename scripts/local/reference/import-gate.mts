// ADR-0001 §6.1 / alternatives-evaluation §3.11 — the import gate.
//
// `musicxml2ly` warnings are not one class. A `Negative skip` means the voice constructor jumped
// *backwards* and appended the event at the wrong position: the timeline is compressed, so the
// rhythm of everything after it is wrong. Alternatives evaluation §3.11 measured that as 81.9 % /
// 59.0 % MIDI alignment and concluded it **must not** be waved through by a human confirmation — the fix is voice
// preprocessing before conversion (ADR-OPEN-2, not implemented). Everything else the converter
// warns about (an unreadable metronome mark, an unsupported MusicXML 4.0 attribute) is a localised
// loss, so the ADR's three outcomes apply: clean import / needs-confirmation / refuse-and-report.
import { buildIndex } from '../../../packages/ly/src/mod.ts';

export type WarningClass = 'blocking' | 'loss';

/** Rule table for the warnings the corpus and the converter actually produce. */
const CLASSIFY: { re: RegExp; cls: WarningClass; note: string }[] = [
  {
    re: /Negative skip/i,
    cls: 'blocking',
    note: '声部时间线被压缩/错位（§3.11）：需要声部预处理后重转，MVP 未实现（ADR-OPEN-2）',
  },
  {
    re: /Invalid bpm/i,
    cls: 'loss',
    note: '节拍器速度标记无法解析：仅影响该速度文本，音高与时值不受影响',
  },
  {
    re: /unknown|not supported|ignoring|skipping element/i,
    cls: 'loss',
    note: '该 MusicXML 特性超出转换器支持子集：已在导入报告中列为损失项',
  },
];

export const classify = (line: string): WarningClass =>
  CLASSIFY.find((c) => c.re.test(line))?.cls ?? 'loss';

export interface ImportReport {
  status: 'clean' | 'needs-confirmation' | 'refused';
  measures: number;
  voices: number;
  noteEvents: number;
  warnings: string[];
  blocking: string[];
  losses: string[];
  notes: string[];
}

/**
 * Three outcomes, never a silent import: counts come from L1 (ADR-0002 §3.2 makes `% N` comments
 * the measure authority, so a `|` count would lie), and the warning lines decide the class.
 */
export function checkImport(ly: string, log: string[]): ImportReport {
  const idx = buildIndex(ly);
  const warnings = log.filter((l) => /warning|error|skip/i.test(l));
  const emptyVoices = idx.voices.filter((v) => !v.measures.length).map((v) => v.name);
  const blocking = warnings.filter((w) => classify(w) === 'blocking');
  const losses = warnings.filter((w) => classify(w) === 'loss');
  const notes: string[] = [];
  for (const w of [...blocking, ...losses]) {
    const note = CLASSIFY.find((c) => c.re.test(w))?.note;
    if (note && !notes.includes(note)) notes.push(note);
  }
  const structural = emptyVoices.length > 0 || idx.voices.length === 0;
  return {
    status:
      blocking.length || structural
        ? blocking.length
          ? 'refused'
          : 'needs-confirmation'
        : losses.length
          ? 'needs-confirmation'
          : 'clean',
    measures: idx.maxMeasures,
    voices: idx.voices.length,
    noteEvents: idx.noteEvents,
    warnings: [...warnings, ...emptyVoices.map((v) => `voice ${v} has no measures`)],
    blocking,
    losses,
    notes,
  };
}
