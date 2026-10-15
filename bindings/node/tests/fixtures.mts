import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { Document } from '../index.js';

export const root = fileURLToPath(new URL('../../../', import.meta.url));
export const modelPath = `${root}assets/en_core_web_md-3.8.0`;
export interface ReferenceToken {
  start: number; end: number; idx: number; whitespace: boolean; norm: string;
  tag: string | null; pos: string | null; morphology: string | null; lemma: string | null;
  head: number | null; dep: string | null; sentence_start: boolean | null;
  entity_iob: string | null; entity_type: string | null;
}
export interface ReferenceSpan { start: number; end: number; label: string }
export interface ReferenceDocument {
  text: string; tokens: ReferenceToken[];
  entities: ReferenceSpan[] | null; sentences: ReferenceSpan[] | null; noun_chunks: ReferenceSpan[] | null;
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
/** Narrow a value to a plain object without copying it, so callers can edit fixtures in place. */
export function record(value: unknown): Record<string, unknown> {
  assert.ok(isRecord(value));
  return value;
}
export function array(value: unknown): unknown[] {
  assert.ok(Array.isArray(value));
  return value;
}
function string(value: unknown): string { assert.ok(typeof value === 'string'); return value; }
function number(value: unknown): number { assert.ok(typeof value === 'number' && Number.isFinite(value)); return value; }
function boolean(value: unknown): boolean { assert.ok(typeof value === 'boolean'); return value; }
function nullable<T>(value: unknown, parse: (value: unknown) => T): T | null { return value === null ? null : parse(value); }
function token(value: unknown): ReferenceToken {
  const t = record(value);
  return { start: number(t.start), end: number(t.end), idx: number(t.idx), whitespace: boolean(t.whitespace), norm: string(t.norm),
    tag: nullable(t.tag, string), pos: nullable(t.pos, string), morphology: nullable(t.morphology, string), lemma: nullable(t.lemma, string),
    head: nullable(t.head, number), dep: nullable(t.dep, string), sentence_start: nullable(t.sentence_start, boolean),
    entity_iob: nullable(t.entity_iob, string), entity_type: nullable(t.entity_type, string) };
}
function spans(value: unknown): ReferenceSpan[] | null {
  return nullable(value, value => array(value).map(value => {
    const s = record(value);
    return { start: number(s.start), end: number(s.end), label: string(s.label) };
  }));
}
export function readJson(path: string): unknown { return JSON.parse(readFileSync(path, 'utf8')); }
export function corpus(name: string): ReferenceDocument[] {
  const fixture = record(readJson(`${root}fixtures/${name}.expected.json`));
  assert.deepEqual(fixture.versions, { spacy: '3.8.14', thinc: '8.3.13' });
  const cases = array(fixture.cases);
  assert.ok(cases.length > 0);
  return cases.map(value => {
    const c = record(value);
    return { text: string(c.text), tokens: array(c.tokens).map(token), entities: spans(c.entities), sentences: spans(c.sentences), noun_chunks: spans(c.noun_chunks) };
  });
}
/** Spans without their IDs, after checking there are none: the official suites predate entity IDs, and pipeline output never has them. */
function withoutIds(spans: Document['entities']): ReferenceSpan[] | null {
  return spans && spans.map(({ id, ...span }) => { assert.equal(id, ''); return span; });
}
export function snapshot(doc: Document): ReferenceDocument {
  assert.ok(doc.tokens.every(t => t.entityId === ''));
  return { text: doc.text, tokens: doc.tokens.map(t => ({ start: t.byteStart, end: t.byteEnd, idx: t.codePointStart,
    whitespace: t.whitespace === ' ', norm: t.norm, tag: t.tag, pos: t.pos, morphology: t.morphology,
    lemma: t.lemma, head: t.head, dep: t.dep, sentence_start: t.sentenceStart, entity_iob: t.entityIob, entity_type: t.entityType })),
    entities: withoutIds(doc.entities), sentences: withoutIds(doc.sentences), noun_chunks: withoutIds(doc.nounChunks) };
}
