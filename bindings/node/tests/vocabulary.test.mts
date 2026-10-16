import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { DependencyMatcher, isOfficialModelName, isSparsError, loadModel, NativeDocument, TokenMatcher } from '../index.js';
import type { DependencyPattern, TokenConstraint, TokenPattern } from '../index.js';
import { modelPath } from './fixtures.mts';
import {
  COMPARISONS, ENTITY_IOB, ERROR_CODES, FLAG_ATTRIBUTES, INTEGER_SET_KINDS, OFFICIAL_MODEL_NAMES, RELATIONS, REPETITIONS,
  STRING_ATTRIBUTES, STRING_SET_KINDS, UNIVERSAL_POS,
} from './vocabulary.mts';

function snapshot(token: { pos?: string; entity_iob?: string }): string {
  return JSON.stringify({ format_version: 1, document: { text: 'a', tokens: [{ start: 0, end: 1, idx: 0, whitespace: false, norm: 'a', ...token }] } });
}

test('snapshots accept exactly the declared POS and IOB values', () => {
  for (const pos of [...UNIVERSAL_POS, '']) {
    assert.equal(NativeDocument.fromSnapshot(snapshot({ pos })).toObject().tokens[0]?.pos, pos);
  }
  // Each value follows an `X` entity, so an `I` has an entity to continue; the stored list is the one the tags describe.
  const entities = { B: [[0, 1], [1, 2]], I: [[0, 2]], O: [[0, 1]], '': [[0, 1]] } as const;
  for (const iob of [...ENTITY_IOB, ''] as const) {
    const tokens = [{ start: 0, end: 1, idx: 0, whitespace: true, norm: 'a', entity_iob: 'B', entity_type: 'X' },
      { start: 2, end: 3, idx: 2, whitespace: false, norm: 'b', entity_iob: iob, entity_type: 'X' }];
    const tagged = JSON.stringify({ format_version: 1, document: { text: 'a b', tokens,
      entities: entities[iob].map(([start, end]) => ({ start, end, label: 'X' })) } });
    assert.equal(NativeDocument.fromSnapshot(tagged).toObject().tokens[1]?.entityIob, iob);
  }
  for (const pos of ['noun', 'Noun', 'FOO']) {
    assert.throws(() => NativeDocument.fromSnapshot(snapshot({ pos })), { code: 'SPARS_INVALID_MODEL' });
  }
  for (const iob of ['b', 'X', 'BI']) {
    assert.throws(() => NativeDocument.fromSnapshot(snapshot({ entity_iob: iob })), { code: 'SPARS_INVALID_MODEL' });
  }
});

test('every declared token condition is accepted and read back by both matchers', async () => {
  const model = await loadModel(modelPath);
  const conditions: TokenConstraint[] = [
    ...STRING_ATTRIBUTES.flatMap((attribute): TokenConstraint[] => [
      { attribute, predicate: { kind: 'equals', value: 'a' } },
      ...STRING_SET_KINDS.map((kind): TokenConstraint => ({ attribute, predicate: { kind, values: ['a'] } })),
    ]),
    ...UNIVERSAL_POS.map((value): TokenConstraint => ({ attribute: 'pos', predicate: { kind: 'equals', value } })),
    { attribute: 'tag', predicate: { kind: 'in', values: ['NNP', ''] } },
    { attribute: 'dep', predicate: { kind: 'equals', value: 'nsubj' } },
    { attribute: 'morphology', predicate: { kind: 'morph_superset', values: ['Number=Sing'] } },
    { attribute: 'morphology', predicate: { kind: 'morph_intersects', values: ['Number=Sing'] } },
    ...FLAG_ATTRIBUTES.map((attribute): TokenConstraint => ({ attribute, predicate: { kind: 'flag', value: true } })),
    ...COMPARISONS.map((operator): TokenConstraint => ({ attribute: 'length', predicate: { kind: 'compare', operator, value: 2 } })),
    ...INTEGER_SET_KINDS.map((kind): TokenConstraint => ({ attribute: 'length', predicate: { kind, values: [2] } })),
  ];
  const tokens = new TokenMatcher(model);
  const dependencies = new DependencyMatcher(model);
  for (const [index, condition] of conditions.entries()) {
    const pattern: TokenPattern = { tokens: [{ constraints: [condition], repetition: { kind: 'once' } }] };
    tokens.add(`t${index}`, [pattern]);
    assert.deepStrictEqual(tokens.get(`t${index}`), [pattern]);
    const node: DependencyPattern = { nodes: [{ id: 'a', constraints: [condition] }] };
    dependencies.add(`d${index}`, [node]);
    assert.deepStrictEqual(dependencies.get(`d${index}`), [node]);
  }
  for (const kind of REPETITIONS) {
    const pattern: TokenPattern = { tokens: [{ constraints: [], repetition: { kind } }] };
    tokens.add(kind, [pattern]);
    assert.deepStrictEqual(tokens.get(kind), [pattern]);
  }
  const range: TokenPattern = { tokens: [{ constraints: [], repetition: { kind: 'range', min: 1, max: 2 } }] };
  tokens.add('range', [range]);
  assert.deepStrictEqual(tokens.get('range'), [range]);
  for (const relation of RELATIONS) {
    const pattern: DependencyPattern = { nodes: [{ id: 'a', constraints: [] }, { id: 'b', constraints: [], link: { left: 'a', relation } }] };
    dependencies.add(relation, [pattern]);
    assert.deepStrictEqual(dependencies.get(relation), [pattern]);
  }
});

test('error codes and official model names match the runtime guards', () => {
  const execution: unknown = createRequire(import.meta.url)('../execution.cjs');
  assert.ok(typeof execution === 'object' && execution !== null && 'ERROR_CODES' in execution && 'OFFICIAL_MODEL_NAMES' in execution);
  assert.deepEqual(execution.ERROR_CODES, new Set(ERROR_CODES));
  assert.deepEqual(execution.OFFICIAL_MODEL_NAMES, new Set(OFFICIAL_MODEL_NAMES));
  for (const code of ERROR_CODES) assert.equal(isSparsError(Object.assign(new Error('x'), { code })), true);
  for (const value of [new Error('x'), Object.assign(new Error('x'), { code: 'ENOENT' }), { code: 'SPARS_IO' }, 'SPARS_IO', null]) {
    assert.equal(isSparsError(value), false);
  }
  for (const name of OFFICIAL_MODEL_NAMES) assert.equal(isOfficialModelName(name), true);
  for (const name of ['en_core_web_xl', 'EN_CORE_WEB_SM', '']) assert.equal(isOfficialModelName(name), false);
});

test('native errors carry declared codes', async () => {
  try {
    NativeDocument.fromSnapshot('{');
    assert.fail('expected an error');
  } catch (error) {
    assert.ok(isSparsError(error));
    assert.ok(ERROR_CODES.includes(error.code));
  }
});
