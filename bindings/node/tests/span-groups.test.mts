import assert from 'node:assert/strict';
import { test } from 'node:test';
import { NativeDocument } from '../index.js';
import type { JsonValue, SpanGroup, SpanGroupAttrs, SpanGroupInput, SpanInput } from '../index.js';
import { array, readJson, record, root } from './fixtures.mts';

type Action = 'set' | 'set_group' | 'delete' | 'append' | 'extend' | 'extend_group' | 'concat' | 'set_item' | 'copy';
type Rejection = 'IndexError' | 'KeyError';
interface SpanRecord { start: number; end: number; label: string; id: string | null }
interface Step {
  action: Action; key: string | null; spans: SpanRecord[] | null; name: string | null; attrs: SpanGroupAttrs | null;
  other: string | null; second: string | null; index: number | null;
}
interface GroupRecord { key: string; name: string; attrs: SpanGroupAttrs; spans: SpanRecord[] }
interface State { error: Rejection | null; groups: GroupRecord[] }
interface Case { id: string; words: string[]; spaces: boolean[]; steps: Step[]; states: State[] }

const ACTIONS: readonly Action[] = ['set', 'set_group', 'delete', 'append', 'extend', 'extend_group', 'concat', 'set_item', 'copy'];
function string(value: unknown): string { assert.ok(typeof value === 'string'); return value; }
function nullableString(value: unknown): string | null { return value === null ? null : string(value); }
function number(value: unknown): number { assert.ok(typeof value === 'number' && Number.isInteger(value)); return value; }
function json(value: unknown): JsonValue {
  if (value === null || typeof value === 'boolean' || typeof value === 'number' || typeof value === 'string') return value;
  if (Array.isArray(value)) return value.map(json);
  return attrs(value);
}
function attrs(value: unknown): SpanGroupAttrs {
  return Object.fromEntries(Object.entries(record(value)).map(([k, v]) => [k, json(v)]));
}
function spanRecord(value: unknown): SpanRecord {
  const s = record(value);
  return { start: number(s.start), end: number(s.end), label: string(s.label), id: nullableString(s.id) };
}
function action(value: unknown): Action {
  const name = ACTIONS.find(a => a === value);
  assert.ok(name, String(value));
  return name;
}
function rejection(value: unknown): Rejection | null {
  if (value === null) return null;
  assert.ok(value === 'IndexError' || value === 'KeyError', String(value));
  return value;
}
function cases(): Case[] {
  const fixture = record(readJson(`${root}fixtures/span-groups-v1.expected.json`));
  assert.equal(fixture.format_version, 1);
  return array(fixture.cases).map(value => {
    const c = record(value);
    return {
      id: string(c.id),
      words: array(c.words).map(string),
      spaces: array(c.spaces).map(v => { assert.ok(typeof v === 'boolean'); return v; }),
      steps: array(c.steps).map(value => {
        const s = record(value);
        return {
          action: action(s.action), key: nullableString(s.key), spans: s.spans === null ? null : array(s.spans).map(spanRecord),
          name: nullableString(s.name), attrs: s.attrs === null ? null : attrs(s.attrs), other: nullableString(s.other),
          second: nullableString(s.second), index: s.index === null ? null : number(s.index),
        };
      }),
      states: array(c.states).map(value => {
        const s = record(value);
        return {
          error: rejection(s.error),
          groups: array(s.groups).map(value => {
            const g = record(value);
            return { key: string(g.key), name: string(g.name), attrs: attrs(g.attrs), spans: array(g.spans).map(spanRecord) };
          }),
        };
      }),
    };
  });
}

function document(words: string[], spaces: boolean[]): NativeDocument {
  let text = '';
  const tokens = words.map((word, i) => {
    const start = Buffer.byteLength(text);
    const idx = [...text].length;
    text += word;
    const token = { start, end: Buffer.byteLength(text), idx, whitespace: spaces[i], norm: word };
    if (spaces[i]) text += ' ';
    return token;
  });
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text, tokens, entities: null, sentences: null, noun_chunks: null } }));
}

const input = (s: SpanRecord): SpanInput => (s.id === null ? { start: s.start, end: s.end, label: s.label } : { ...s, id: s.id });
const stored = (g: SpanGroup): SpanGroupInput => ({ name: g.name, attrs: g.attrs, spans: g.spans });

const bindingRejections = { bounds: 0, missingKey: 0 };

class Rejected extends Error {
  readonly kind: Rejection;
  constructor(kind: Rejection) { super(kind); this.kind = kind; }
}

function group(doc: NativeDocument, key: string | null): SpanGroup {
  const found = doc.toObject().spanGroups.find(g => g.key === key);
  if (!found) throw new Rejected('KeyError');
  return found;
}

/** Store through the binding, mapping its bounds error to spaCy's IndexError. */
function store(doc: NativeDocument, key: string, value: SpanGroupInput): NativeDocument {
  try {
    return doc.withSpanGroup(key, value);
  } catch (error) {
    if (record(error).code === 'SPARS_BOUNDS') { bindingRejections.bounds++; throw new Rejected('IndexError'); }
    throw error;
  }
}

/**
 * The merged attrs of spaCy's `SpanGroup._concat`: the target's keys, then those it lacks. Node has no extend or concat,
 * so these steps check that the binding stores the merged group; the Rust replay checks the native merge.
 */
function merged(target: SpanGroupAttrs, source: SpanGroupAttrs): SpanGroupAttrs {
  return { ...target, ...Object.fromEntries(Object.entries(source).filter(([k]) => !(k in target))) };
}

function run(doc: NativeDocument, step: Step): NativeDocument {
  const key = step.key ?? '';
  const spans = (step.spans ?? []).map(input);
  switch (step.action) {
    case 'set': return store(doc, key, { spans });
    case 'set_group': return store(doc, key, { name: step.name ?? '', attrs: step.attrs ?? {}, spans });
    case 'delete':
      try {
        return doc.withoutSpanGroup(key);
      } catch (error) {
        assert.equal(record(error).code, 'InvalidArg');
        bindingRejections.missingKey++;
        throw new Rejected('KeyError');
      }
    case 'append':
    case 'extend': {
      const target = group(doc, key);
      return store(doc, key, { ...stored(target), spans: [...target.spans, ...spans] });
    }
    case 'extend_group': {
      const target = group(doc, key);
      const source = group(doc, step.other);
      return store(doc, key, { name: target.name, attrs: merged(target.attrs, source.attrs), spans: [...target.spans, ...source.spans] });
    }
    case 'concat': {
      const first = group(doc, step.other);
      const second = group(doc, step.second);
      return store(doc, key, { name: first.name, attrs: merged(first.attrs, second.attrs), spans: [...first.spans, ...second.spans] });
    }
    case 'set_item': {
      // Python builds the new span, checking its bounds, before looking up the key and index.
      const length = doc.toObject().tokens.length;
      if (spans.some(s => s.start > s.end || s.end > length)) throw new Rejected('IndexError');
      const target = group(doc, key);
      const index = step.index ?? 0;
      const position = index < 0 ? index + target.spans.length : index;
      if (position < 0 || position >= target.spans.length) throw new Rejected('IndexError');
      const next: SpanInput[] = [...target.spans];
      const replacement = spans[0];
      assert.ok(replacement);
      next[position] = replacement;
      return store(doc, key, { ...stored(target), spans: next });
    }
    case 'copy': return NativeDocument.fromSnapshot(doc.toSnapshot());
  }
}

/** Output with attr key order made explicit, since deepEqual ignores property order. */
function observed(doc: NativeDocument): unknown {
  return doc.toObject().spanGroups.map(g => ({
    key: g.key, name: g.name, attrs: Object.entries(g.attrs), spans: g.spans.map(s => ({ start: s.start, end: s.end, label: s.label, id: s.id || null })),
  }));
}
function expected(state: State): unknown {
  return state.groups.map(g => ({ key: g.key, name: g.name, attrs: Object.entries(g.attrs), spans: g.spans }));
}

test('official span group edits replay through native documents', () => {
  let steps = 0;
  // Rejections raised by the binding itself are counted apart from those the harness derives.
  const reached = { IndexError: 0, KeyError: 0, unsortedAttrs: 0, bindingBounds: 0, bindingMissingKey: 0 };
  for (const c of cases()) {
    let doc = document(c.words, c.spaces);
    for (const [index, step] of c.steps.entries()) {
      const state = c.states[index];
      assert.ok(state);
      const at = `${c.id} step ${index}`;
      const before = doc;
      const snapshot = doc.toSnapshot();
      let error: Rejection | null = null;
      try {
        doc = run(doc, step);
      } catch (thrown) {
        if (!(thrown instanceof Rejected)) throw thrown;
        error = thrown.kind;
        reached[error]++;
      }
      assert.equal(error, state.error, at);
      // Documents are immutable: an edit returns a new document and leaves the old one as it was.
      assert.equal(before.toSnapshot(), snapshot, `${at}: the source document is unchanged`);
      assert.deepEqual(observed(doc), expected(state), at);
      assert.deepEqual(observed(NativeDocument.fromSnapshot(doc.toSnapshot())), expected(state), `${at}: snapshot round trip`);
      reached.unsortedAttrs += state.groups.filter(g => Object.keys(g.attrs).join() !== Object.keys(g.attrs).sort().join()).length;
      steps++;
    }
  }
  assert.equal(steps, 412);
  reached.bindingBounds = bindingRejections.bounds;
  reached.bindingMissingKey = bindingRejections.missingKey;
  for (const [name, count] of Object.entries(reached)) assert.ok(count > 0, name);
});

test('withSpanGroup defaults the name to the key and the label to empty, and checks bounds', () => {
  const doc = document(['New', 'York', 'City'], [true, true, false]);
  const edited = doc.withSpanGroup('places', { spans: [{ start: 0, end: 3, label: 'GPE', id: 'nyc' }, { start: 1, end: 1 }] });
  assert.deepEqual(edited.toObject().spanGroups, [{ key: 'places', name: 'places', attrs: {}, spans: [
    { start: 0, end: 3, label: 'GPE', id: 'nyc' }, { start: 1, end: 1, label: '', id: '' },
  ] }]);
  assert.deepEqual(doc.toObject().spanGroups, []);
  // Replacing a key keeps its position; a removed key that is set again goes last.
  const two = edited.withSpanGroup('other', { name: 'n', spans: [] }).withSpanGroup('places', { spans: [], name: null, attrs: null });
  assert.deepEqual(two.toObject().spanGroups, [{ key: 'places', name: 'places', attrs: {}, spans: [] }, { key: 'other', name: 'n', attrs: {}, spans: [] }]);
  // An explicit empty name is kept, as in spaCy.
  assert.equal(doc.withSpanGroup('g', { name: '', spans: [] }).toObject().spanGroups[0]?.name, '');
  // An empty ID means no ID, so the snapshot omits it.
  const unnamed = doc.withSpanGroup('g', { spans: [{ start: 0, end: 1, id: '' }] });
  assert.equal(unnamed.toObject().spanGroups[0]?.spans[0]?.id, '');
  assert.doesNotMatch(unnamed.toSnapshot(), /"id"/);
  assert.deepEqual(two.withoutSpanGroup('places').withSpanGroup('places', { spans: [] }).toObject().spanGroups.map(g => g.key), ['other', 'places']);
  for (const spans of [[{ start: 0, end: 4 }], [{ start: 2, end: 1 }], [{ start: -1, end: 1 }], [{ start: 0.5, end: 1 }]]) {
    assert.throws(() => doc.withSpanGroup('g', { spans }), { code: 'SPARS_BOUNDS' });
  }
  assert.throws(() => doc.withoutSpanGroup('missing'), { code: 'InvalidArg' });
});

test('withSpanGroup keeps attrs order and rejects values that are not plain JSON', () => {
  const doc = document(['a'], [false]);
  const value: SpanGroupAttrs = { z: 1, a: [true, null, 'x', { y: 0.5, b: -2 }], ok: { nested: {} } };
  const attrs = doc.withSpanGroup('g', { attrs: value, spans: [] }).toObject().spanGroups[0]?.attrs;
  assert.deepEqual(attrs, value);
  assert.deepEqual(Object.keys(attrs ?? {}), ['z', 'a', 'ok']);
  const circular: Record<string, JsonValue> = {};
  Object.assign(circular, { self: circular });
  const getter = Object.defineProperty({}, 'k', { get: () => 1, enumerable: true });
  const holes: JsonValue[] = [];
  holes[1] = 1;
  const invalid: unknown[] = [
    [], 1, 'text', { u: undefined }, { n: Number.NaN }, { i: Infinity }, { d: new Date(0) }, { m: new Map() }, { f: () => 1 },
    { b: 1n }, circular, getter, { h: holes }, { [Symbol('s')]: 1 }, Object.create({ inherited: 1 }), { c: new (class Value {})() },
  ];
  for (const attrs of invalid) {
    assert.throws(() => Reflect.apply(doc.withSpanGroup, doc, ['g', { attrs, spans: [] }]), TypeError, String(attrs));
  }
  for (const attrs of [{ k: '\ud800' }, { '\udc00': 1 }, { k: ['ok', { deep: '\ud800' }] }]) {
    assert.throws(() => doc.withSpanGroup('g', { attrs, spans: [] }), { code: 'SPARS_INVALID_TEXT' });
  }
  // Shared, non-circular references are plain JSON.
  const shared = { s: 1 };
  assert.deepEqual(doc.withSpanGroup('g', { attrs: { a: shared, b: shared }, spans: [] }).toObject().spanGroups[0]?.attrs, { a: { s: 1 }, b: { s: 1 } });
  // A prototype-less object is plain JSON too.
  const bare: SpanGroupAttrs = Object.assign(Object.create(null), { k: 1 });
  assert.deepEqual(doc.withSpanGroup('g', { attrs: bare, spans: [] }).toObject().spanGroups[0]?.attrs, { k: 1 });
});

test('withSpanGroup rejects unknown fields, wrong types and invalid text', () => {
  const doc = document(['a', 'b'], [true, false]);
  const unchecked = (key: unknown, group: unknown): unknown => Reflect.apply(doc.withSpanGroup, doc, [key, group]);
  for (const group of [{ spans: [], extra: 1 }, { spans: [{ start: 0, end: 1, kbId: 'Q1' }] }]) {
    assert.throws(() => unchecked('g', group), { code: 'InvalidArg' });
  }
  for (const [key, group] of [[1, { spans: [] }], ['g', null], ['g', []], ['g', 'x'], ['g', { spans: 'x' }], ['g', {}], ['g', { name: 1, spans: [] }],
    ['g', { spans: [{ start: 0, end: 1, label: 1 }] }], ['g', { spans: [{ start: '0', end: 1 }] }], ['g', { spans: [1] }]] as const) {
    assert.throws(() => unchecked(key, group), { code: 'InvalidArg' }, JSON.stringify(group));
  }
  assert.throws(() => unchecked('g', { spans: [null] }), TypeError);
  for (const span of [{ start: Number.NaN, end: 1 }, { start: 0, end: Infinity }, { start: 0, end: 2 ** 32 }]) {
    assert.throws(() => unchecked('g', { spans: [span] }), { code: 'SPARS_BOUNDS' }, String(span.end));
  }
  // Attrs are read once, so a getter cannot pass the check and then return other values.
  let reads = 0;
  const changing = { spans: [], get attrs(): SpanGroupAttrs { reads++; return reads > 1 ? { d: 'changed' } : {}; } };
  assert.deepEqual(doc.withSpanGroup('g', changing).toObject().spanGroups[0]?.attrs, {});
  assert.equal(reads, 1);
  for (const [key, group] of [['\ud800', { spans: [] }], ['g', { name: '\ud800', spans: [] }],
    ['g', { spans: [{ start: 0, end: 1, label: '\ud800' }] }], ['g', { spans: [{ start: 0, end: 1, id: '\udc00' }] }]] as const) {
    assert.throws(() => unchecked(key, group), { code: 'SPARS_INVALID_TEXT' });
  }
  assert.throws(() => doc.withoutSpanGroup('\ud800'), { code: 'SPARS_INVALID_TEXT' });
});

test('span groups leave views of the source document valid and survive snapshots', () => {
  const doc = document(['New', 'York'], [true, false]);
  const token = doc.toObject().tokens[0];
  const edited = doc.withSpanGroup('ruler', { attrs: { '1': 'integer-like keys come first in JavaScript', b: 2 }, spans: [{ start: 0, end: 2, label: 'GPE' }] });
  assert.deepEqual(doc.toObject().tokens[0], token);
  const restored = NativeDocument.fromSnapshot(edited.toSnapshot());
  assert.deepEqual(restored.toObject(), edited.toObject());
  assert.deepEqual(Object.keys(restored.toObject().spanGroups[0]?.attrs ?? {}), ['1', 'b']);
  assert.match(edited.toSnapshot(), /"format_version":4/);
  assert.match(edited.withoutSpanGroup('ruler').toSnapshot(), /"format_version":1/);
});
