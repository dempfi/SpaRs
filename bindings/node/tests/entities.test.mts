import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { before, test } from 'node:test';
import { loadModel, NativeDocument } from '../index.js';
import type { EntityDefault, EntityUpdate, Model } from '../index.js';
import { array, modelPath, record, root } from './fixtures.mts';

interface TokenEntity { iob: string | null; type: string | null }
interface EntitySpan { start: number; end: number; label: string }
interface State { error: string | null; tokens: TokenEntity[]; entities: EntitySpan[] | null }
interface Case { id: string; words: string[]; spaces: boolean[]; initial: State; updates: EntityUpdate[]; states: State[] }

function string(value: unknown): string { assert.ok(typeof value === 'string'); return value; }
function nullableString(value: unknown): string | null { return value === null ? null : string(value); }
function integer(value: unknown): number { assert.ok(typeof value === 'number' && Number.isSafeInteger(value)); return value; }
function span(value: unknown): EntitySpan {
  const s = record(value);
  return { start: integer(s.start), end: integer(s.end), label: string(s.label) };
}
function range(value: unknown): { start: number; end: number } {
  const r = record(value);
  return { start: integer(r.start), end: integer(r.end) };
}
const DEFAULTS: readonly EntityDefault[] = ['outside', 'missing', 'blocked', 'unmodified'];
function entityDefault(value: unknown): EntityDefault {
  const found = DEFAULTS.find(d => d === value);
  assert.ok(found);
  return found;
}
function state(value: unknown): State {
  const s = record(value);
  return {
    error: nullableString(s.error),
    tokens: array(s.tokens).map(t => { const r = record(t); return { iob: nullableString(r.iob), type: nullableString(r.type) }; }),
    entities: s.entities === null ? null : array(s.entities).map(span),
  };
}
function update(value: unknown): EntityUpdate {
  const u = record(value);
  return { entities: array(u.entities).map(span), blocked: array(u.blocked).map(range), missing: array(u.missing).map(range),
    outside: array(u.outside).map(range), default: entityDefault(u.default) };
}
function cases(): Case[] {
  const fixture = record(JSON.parse(readFileSync(`${root}fixtures/entity-updates-v1.expected.json`, 'utf8')));
  return array(fixture.cases).map(value => {
    const c = record(value);
    return { id: string(c.id), words: array(c.words).map(string), spaces: array(c.spaces).map(s => { assert.ok(typeof s === 'boolean'); return s; }),
      initial: state(c.initial), updates: array(c.updates).map(update), states: array(c.states).map(state) };
  });
}

/** A restored document with the case's words and initial entity annotation. */
function documentFor(c: Case): NativeDocument {
  let text = '';
  let points = 0;
  const tokens = c.words.map((word, i) => {
    const start = Buffer.byteLength(text);
    const idx = points;
    text += word;
    points += [...word].length;
    const entity = c.initial.tokens[i];
    assert.ok(entity);
    const token = { start, end: Buffer.byteLength(text), idx, whitespace: c.spaces[i], norm: word, tag: null, pos: null,
      morphology: null, lemma: null, head: null, dep: null, sentence_start: null, entity_iob: entity.iob, entity_type: entity.type };
    if (c.spaces[i]) { text += ' '; points++; }
    return token;
  });
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text, tokens, entities: c.initial.entities } }));
}

function observed(doc: NativeDocument): State {
  const output = doc.toObject();
  // entity-updates-v1 predates entity IDs and sets none; entity-ids.test.mts covers them.
  assert.ok(output.tokens.every(t => t.entityId === ''));
  const entities = output.entities && output.entities.map(({ id, ...entity }) => { assert.equal(id, ''); return entity; });
  return { error: null, tokens: output.tokens.map(t => ({ iob: t.entityIob, type: t.entityType })), entities };
}

let model: Model;
before(async () => { model = await loadModel(modelPath); });

test('withEntities matches every frozen spaCy Doc.set_ents update', () => {
  let count = 0;
  for (const c of cases()) {
    let doc = documentFor(c);
    assert.deepEqual(observed(doc), c.initial, c.id);
    c.updates.forEach((u, step) => {
      const expected = c.states[step];
      assert.ok(expected);
      const before = doc.toSnapshot();
      if (expected.error === null) {
        doc = doc.withEntities(u);
      } else {
        assert.throws(() => doc.withEntities(u), { code: 'SPARS_BOUNDS' }, `${c.id} ${step}`);
        assert.equal(doc.toSnapshot(), before);
      }
      assert.deepEqual(observed(doc), { ...expected, error: null }, `${c.id} update ${step}`);
      assert.deepEqual(NativeDocument.fromSnapshot(doc.toSnapshot()).toObject(), doc.toObject());
      count++;
    });
  }
  assert.equal(count, 330);
});

test('withEntities returns a copy and leaves the document and its views unchanged', async () => {
  const doc = await model.processDocument('Tim Cook visited London.');
  const token = doc.token(3);
  const span = doc.span(0, 2);
  const before = doc.toSnapshot();
  const edited = doc.withEntities({ entities: [{ start: 3, end: 4, label: 'CITY' }], default: 'unmodified' });
  assert.equal(doc.toSnapshot(), before);
  assert.equal(token.annotations().entityType, 'GPE');
  assert.equal(span.text, 'Tim Cook');
  assert.deepEqual(edited.toObject().entities, [{ start: 0, end: 2, label: 'PERSON', id: '' }, { start: 3, end: 4, label: 'CITY', id: '' }]);
  assert.equal(edited.token(3).annotations().entityType, 'CITY');
  // Everything except entity annotation is preserved, including contextual vectors.
  const strip = (json: string): unknown => {
    const document = record(record(JSON.parse(json)).document);
    return { ...document, entities: null, tokens: array(document.tokens).map(t => ({ ...record(t), entity_iob: null, entity_type: null })) };
  };
  assert.deepEqual(strip(edited.toSnapshot()), strip(before));
  assert.equal(edited.utf16Length, doc.utf16Length);
  assert.deepEqual(NativeDocument.fromSnapshot(edited.toSnapshot()).toObject(), edited.toObject());
  assert.equal(doc.withEntities({ default: 'missing' }).toObject().entities, null);
  const outside = doc.withEntities({}).toObject();
  assert.deepEqual(outside.entities, []);
  assert.ok(outside.tokens.every(token => token.entityIob === 'O' && token.entityType === ''));
});

test('withEntities rejects invalid intervals, defaults and labels', async () => {
  const doc = await model.processDocument('Tim Cook visited London.');
  for (const bad of [-1, 0.5, Number.NaN, Infinity, 2 ** 32]) {
    assert.throws(() => doc.withEntities({ entities: [{ start: bad, end: 1, label: 'X' }] }), { code: 'SPARS_BOUNDS' });
    assert.throws(() => doc.withEntities({ outside: [{ start: 0, end: bad }] }), { code: 'SPARS_BOUNDS' });
  }
  assert.throws(() => doc.withEntities({ entities: [{ start: 0, end: 6, label: 'X' }] }), { code: 'SPARS_BOUNDS' });
  assert.throws(() => doc.withEntities({ entities: [{ start: 2, end: 1, label: 'X' }] }), { code: 'SPARS_BOUNDS' });
  assert.throws(() => doc.withEntities({ entities: [{ start: 0, end: 2, label: 'X' }], blocked: [{ start: 1, end: 2 }] }),
    { code: 'SPARS_BOUNDS' });
  assert.throws(() => doc.withEntities({ entities: [{ start: 0, end: 1, label: '\ud800' }] }), { code: 'SPARS_INVALID_TEXT' });
  for (const malformed of [
    null, { default: 'keep' }, { entities: {} }, { blocked: 'x' }, { outside: [{ start: 0 }] },
    { entities: [{ start: 0, end: 1 }] }, { entities: [{ start: '1', end: 1, label: 'X' }] },
    { entities: [{ start: 0, end: 1, label: 3 }] }, { missing: [{ start: 0, end: 1.5 }] },
  ]) {
    assert.throws(() => Reflect.apply(doc.withEntities, doc, [malformed]),
      (error: unknown) => error instanceof Error && 'code' in error && ['InvalidArg', 'NumberExpected', 'StringExpected', 'SPARS_BOUNDS'].includes(String(error.code)),
      JSON.stringify(malformed));
  }
  // Undeclared fields are rejected, so a misspelling cannot turn into the default update.
  for (const unknown of [{ ents: [] }, { entities: [{ start: 0, end: 1, label: 'X', kbId: 'Q1' }] }, { outside: [{ start: 0, end: 1, label: 'X' }] }]) {
    assert.throws(() => Reflect.apply(doc.withEntities, doc, [unknown]), { code: 'InvalidArg', message: /unknown entity update field/ });
  }
});

test('withEntities and snapshots reject tags that spaCy could not read', () => {
  const token = (iob: string | null, start: number) => ({ start, end: start + 1, idx: start, whitespace: false, norm: 'a', tag: null, pos: null,
    morphology: null, lemma: null, head: null, dep: null, sentence_start: null, entity_iob: iob, entity_type: iob === null ? null : 'X' });
  const snapshot = (first: string): string => JSON.stringify({ format_version: 1, document: { text: 'aa', tokens: [token(first, 0), token(null, 1)], entities: null } });
  // An I with no entity to continue is a valid tag, but spaCy could not read the entity list.
  const doc = NativeDocument.fromSnapshot(snapshot('I'));
  assert.throws(() => doc.withEntities({ entities: [{ start: 1, end: 2, label: 'Y' }], default: 'unmodified' }), { code: 'SPARS_UNSUPPORTED' });
  // A tag outside B, I, O and empty is rejected when the snapshot is read.
  assert.throws(() => NativeDocument.fromSnapshot(snapshot('X')), { code: 'SPARS_INVALID_MODEL' });
});
