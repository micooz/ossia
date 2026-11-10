import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../../', import.meta.url));
export const fixtures = path.join(root, 'tests/fixtures/local');
export const hash = (bytes: Uint8Array | string) =>
  createHash('sha256').update(bytes).digest('hex');
export const json = (value: unknown) => `${JSON.stringify(value, null, 2)}\n`;
export function write(file: string, value: string | Uint8Array) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, value);
}
export function readSource(file: string) {
  const bytes = fs.readFileSync(file);
  const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  return { text, bytes, sourceHash: hash(bytes) };
}
export function position(source: string, offset: number) {
  if (offset < 0 || offset > source.length) throw new Error('offset outside source');
  const before = source.charCodeAt(offset - 1);
  const after = source.charCodeAt(offset);
  if (before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff)
    throw new Error('offset splits surrogate pair');
  const prefix = source.slice(0, offset);
  const lineStart = prefix.lastIndexOf('\n') + 1;
  return {
    byte: Buffer.byteLength(prefix),
    line: prefix.split('\n').length,
    column: Array.from(prefix.slice(lineStart)).length + 1,
  };
}
export function span(source: string, from: number, to: number) {
  const start = position(source, from);
  const end = position(source, to);
  return { byteStart: start.byte, byteEnd: end.byte, start, end };
}
export function scrub(text: string, scratch?: string) {
  let out = text;
  if (scratch) out = out.replaceAll(scratch, '<scratch>');
  return out.replaceAll(root, '<repo>/');
}
