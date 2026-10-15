import assert from 'node:assert/strict';
import { test } from 'node:test';
import { NativeDocument } from '../index.js';
import type { EntityUpdate } from '../index.js';
import { array, readJson, record, root } from './fixtures.mts';

interface TokenEntity { iob: string | null; type: string | null; id: string }
interface EntitySpan { start: number; end: number; label: string; id: string }
interface State { error: string | null; tokens: TokenEntity[]; entities: EntitySpan[] | null }
interface Case { id: string; words: string[]; spaces: boolean[]; initial: State; updates: EntityUpdate[]; states: State[] }

function string(value: unknown): string { assert.ok(typeof value === 'string'); return value; }
function nullableString(value: unknown): string | null { return value === null ? null : string(value); }
function integer(value: unknown): number { assert.ok(typeof value === 'number' && Number.isSafeInteger(value)); return value; }
function boolean(value: unknown): boolean { assert.ok(typeof value === 'boolean'); return value; }
/** The fixture records the empty ID as null, as spaCy's empty string; Node outputs ''. */
function id(value: unknown): string { return nullableString(value) ?? ''; }
function span(value: unknown): EntitySpan {
  const s = record(value);
  return { start: integer(s.start), end: integer(s.end), label: string(s.label), id: id(s.id) };
}
function range(value: unknown): { start: number; end: number } {
  const r = record(value);
  return { start: integer(r.start), end: integer(r.end) };
}
const DEFAULTS = ['outside', 'missing', 'blocked', 'unmodified'] as const;
function update(value: unknown): EntityUpdate {
  const u = record(value);
  const fallback = DEFAULTS.find(d => d === u.default);
  assert.ok(fallback !== undefined);
  // Entities without an ID leave the field out, as a caller would.
  const entities = array(u.entities).map(span).map(({ id, ...entity }) => (id ? { ...entity, id } : entity));
  return { entities, blocked: array(u.blocked).map(range), missing: array(u.missing).map(range),
    outside: array(u.outside).map(range), default: fallback };
}
function state(value: unknown): State {
  const s = record(value);
  return { error: nullableString(s.error),
    tokens: array(s.tokens).map(t => { const r = record(t); return { iob: nullableString(r.iob), type: nullableString(r.type), id: id(r.id) }; }),
    entities: s.entities === null ? null : array(s.entities).map(span) };
}
function cases(): Case[] {
  return array(record(readJson(`${root}fixtures/entity-ids-v1.expected.json`)).cases).map(value => {
    const c = record(value);
    return { id: string(c.id), words: array(c.words).map(string), spaces: array(c.spaces).map(boolean),
      initial: state(c.initial), updates: array(c.updates).map(update), states: array(c.states).map(state) };
  });
}

function initial(c: Case): NativeDocument {
  let text = '';
  const tokens = c.words.map((word, i) => {
    const entity = c.initial.tokens[i];
    const space = c.spaces[i];
    assert.ok(entity !== undefined && space !== undefined);
    const start = Buffer.byteLength(text);
    const idx = [...text].length;
    text += word;
    const token = { start, end: Buffer.byteLength(text), idx, whitespace: space, norm: word,
      entity_iob: entity.iob, entity_type: entity.type, entity_id: entity.id || undefined };
    if (space) text += ' ';
    return token;
  });
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 3, document: { text, tokens, entities: c.initial.entities?.map(
    ({ id, ...entity }) => (id ? { ...entity, id } : entity)) ?? null } }));
}

function observed(doc: NativeDocument): State {
  const output = doc.toObject();
  return { error: null, tokens: output.tokens.map(t => ({ iob: t.entityIob, type: t.entityType, id: t.entityId })), entities: output.entities };
}

test('withEntities matches every frozen spaCy entity ID update', () => {
  let count = 0;
  for (const c of cases()) {
    let doc = initial(c);
    assert.deepEqual(observed(doc), c.initial, `${c.id} initial`);
    assert.equal(c.updates.length, c.states.length, c.id);
    for (const [step, u] of c.updates.entries()) {
      const expected = c.states[step];
      assert.ok(expected !== undefined);
      const context = `${c.id} update ${step}`;
      if (expected.error) {
        assert.throws(() => doc.withEntities(u), { code: 'SPARS_BOUNDS' }, context);
      } else {
        doc = doc.withEntities(u);
        // Snapshots keep the IDs, so a restored document continues the same sequence.
        doc = NativeDocument.fromSnapshot(doc.toSnapshot());
      }
      assert.deepEqual(observed(doc), { ...expected, error: null }, context);
      count += 1;
    }
  }
  assert.equal(count, 193);
});

test('token views report entity IDs, and an empty or absent ID keeps the current one', () => {
  const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text: 'a b', tokens: [
    { start: 0, end: 1, idx: 0, whitespace: true, norm: 'a' }, { start: 2, end: 3, idx: 2, whitespace: false, norm: 'b' }] } }));
  const tagged = doc.withEntities({ entities: [{ start: 0, end: 2, label: 'X', id: 'q1' }] });
  assert.equal(tagged.token(1).annotations().entityId, 'q1');
  for (const entity of [{ start: 0, end: 1, label: 'Y' }, { start: 0, end: 1, label: 'Y', id: '' }]) {
    const relabeled = tagged.withEntities({ entities: [entity] });
    assert.deepEqual(relabeled.toObject().entities, [{ start: 0, end: 1, label: 'Y', id: 'q1' }]);
    assert.equal(relabeled.token(1).annotations().entityId, 'q1');
  }
  assert.equal(doc.token(0).annotations().entityId, '');
  assert.match(tagged.toSnapshot(), /"format_version":3/);
  // Knowledge-base IDs are not stored, so the field is rejected rather than dropped.
  const malformed: unknown = { entities: [{ start: 0, end: 1, label: 'X', kbId: 'Q1' }] };
  assert.throws(() => Reflect.apply(doc.withEntities, doc, [malformed]), { code: 'InvalidArg' });
});

test('entity IDs keep any Unicode text exactly and reject unpaired surrogates', () => {
  const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text: 'a', tokens: [
    { start: 0, end: 1, idx: 0, whitespace: false, norm: 'a' }] } }));
  const id = 'Zürich-😀';
  const tagged = NativeDocument.fromSnapshot(doc.withEntities({ entities: [{ start: 0, end: 1, label: 'GPE', id }] }).toSnapshot());
  assert.equal(tagged.token(0).annotations().entityId, id);
  assert.equal(tagged.toObject().entities?.[0]?.id, id);
  assert.throws(() => doc.withEntities({ entities: [{ start: 0, end: 1, label: 'GPE', id: '\ud800' }] }), { code: 'SPARS_INVALID_TEXT' });
});
