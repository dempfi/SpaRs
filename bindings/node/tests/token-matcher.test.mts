import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { loadModel, NativeDocument, TokenMatcher } from '../index.js';
import { entitiesFromTags, modelPath, referenceTokens } from './fixtures.mts';
import type { TokenPattern, TokenConstraint, TokenRepetition } from '../index.js';
import { recordedConstraint, repetition } from './vocabulary.mts';
import type { Recorded } from './vocabulary.mts';
function record(value: unknown): Record<string, unknown> {
  assert.ok(value !== null && typeof value === 'object' && !Array.isArray(value));
  return Object.fromEntries(Object.entries(value));
}
function array(value: unknown): unknown[] { assert.ok(Array.isArray(value)); return value; }
function string(value: unknown): string { assert.equal(typeof value, 'string'); return String(value); }
function number(value: unknown): number { assert.ok(typeof value === 'number' && Number.isSafeInteger(value)); return value; }
function pattern(value: unknown): Recorded<TokenPattern> {
  let typed = true;
  const tokens = array(record(value).tokens).map(value => {
    const item = record(value);
    const constraints = array(item.constraints).map(recordedConstraint);
    typed &&= constraints.every(c => c.typed);
    return { constraints, repetition: repetition(item.repetition) };
  });
  if (typed) {
    return { typed: true, pattern: { tokens: tokens.map(t => ({ constraints: t.constraints.flatMap(c => c.typed ? [c.constraint] : []), repetition: t.repetition })) } };
  }
  return { typed: false, pattern: { tokens: tokens.map(t => ({ constraints: t.constraints.map(c => c.constraint), repetition: t.repetition })) } };
}
/** Add recorded patterns, using an untyped call only for values the declarations deliberately exclude. */
function addRecorded(matcher: TokenMatcher, rule: string, patterns: Recorded<TokenPattern>[]): number {
  const typed = patterns.flatMap(p => p.typed ? [p.pattern] : []);
  if (typed.length === patterns.length) {
    matcher.add(rule, typed);
    return 0;
  }
  addMalformed(matcher, rule, patterns.map(p => p.pattern));
  return 1;
}
/** Pass a deliberately malformed pattern past the declared types. */
function addMalformed(matcher: TokenMatcher, rule: string, patterns: unknown[]): void {
  Reflect.apply(matcher.add, matcher, [rule, patterns]);
}
/** A one-item pattern with an untyped condition, for malformed-input tests. */
function untypedItem(attribute: unknown, predicate: unknown, repetition: unknown = { kind: 'once' }): unknown {
  return { tokens: [{ constraints: [{ attribute, predicate }], repetition }] };
}
function hasCode(code: string): (error: unknown) => boolean {
  return (error: unknown): boolean => error instanceof Error && 'code' in error && error.code === code;
}
function plainDoc(text: string): NativeDocument {
  let start = 0;
  const tokens = [...text].map((norm, idx) => {
    const previous = start;
    start += Buffer.byteLength(norm);
    return { start: previous, end: start, idx, norm, whitespace: false };
  });
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text, tokens } }));
}
function wildcard(repetition: TokenRepetition = { kind: 'once' }): TokenPattern {
  return { tokens: [{ constraints: [], repetition }] };
}
// The last column counts rules with POS values outside the universal tags; see vocabulary.mts.
for (const [filename, cases, rules, matches, untyped] of [
  ['token-match-v1.expected.json', 10, 480, 704, 0],
  ['token-match-exhaustive-v1.expected.json', 31, 4805, 6261, 0],
  ['token-match-branching-v1.expected.json', 31, 248, 288, 0],
  ['token-match-lower-v1.expected.json', 4, 172, 280, 0],
  ['token-match-length-v1.expected.json', 3, 105, 386, 0],
  ['token-match-sets-v1.expected.json', 3, 558, 653, 9],
] as const) {
  test(`TokenMatcher preserves frozen ordered reference corpus ${filename}`, async () => {
    const parsed: unknown = JSON.parse(readFileSync(new URL(`../../../fixtures/${filename}`, import.meta.url), 'utf8'));
    const items = array(record(parsed).cases).map(record);
    assert.equal(items.length, cases);
    let ruleCount = 0;
    let outOfVocabulary = 0;
    let matchCount = 0;
    for (const item of items) {
      const matcher = new TokenMatcher();
      const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { ...item, entities: entitiesFromTags(referenceTokens(item.tokens)) } }));
      for (const raw of array(item.rules)) {
        const rule = record(raw);
        outOfVocabulary += addRecorded(matcher, string(rule.name), array(rule.patterns).map(pattern));
        ruleCount++;
      }
      const expected = array(item.expected).map(value => {
        const match = record(value);
        return { rule: string(match.rule), start: number(match.start), end: number(match.end) };
      });
      matchCount += expected.length;
      assert.equal(matcher.size, new Set(array(item.rules).map(value => string(record(value).name))).size);
      for (const actual of await Promise.all([matcher.findMatches(doc), matcher.findMatches(doc)])) {
        assert.deepEqual(actual, expected, string(item.id));
      }
    }
    assert.equal(ruleCount, rules);
    assert.equal(outOfVocabulary, untyped);
    assert.equal(matchCount, matches);
  });
}

const FLAG_RULES = 51;
const FLAG_MATCHES = 442;
test('TokenMatcher lexical flags match the frozen official suite with a model lexicon', async () => {
  const model = await loadModel(modelPath);
  const parsed: unknown = JSON.parse(readFileSync(new URL('../../../fixtures/token-match-flags-v1.expected.json', import.meta.url), 'utf8'));
  const items = array(record(parsed).cases).map(record);
  assert.equal(items.length, 3);
  let ruleCount = 0;
  let outOfVocabulary = 0;
  let matchCount = 0;
  for (const item of items) {
    const matcher = new TokenMatcher(model);
    const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { ...item, entities: entitiesFromTags(referenceTokens(item.tokens)) } }));
    for (const raw of array(item.rules)) {
      const rule = record(raw);
      outOfVocabulary += addRecorded(matcher, string(rule.name), array(rule.patterns).map(pattern));
      ruleCount++;
    }
    const expected = array(item.expected).map(value => {
      const match = record(value);
      return { rule: string(match.rule), start: number(match.start), end: number(match.end) };
    });
    matchCount += expected.length;
    assert.deepEqual(await matcher.findMatches(doc), expected, string(item.id));
  }
  assert.equal(ruleCount, FLAG_RULES);
  assert.equal(outOfVocabulary, 0);
  assert.equal(matchCount, FLAG_MATCHES);
  const plain = new TokenMatcher();
  const flag = (value: boolean): TokenPattern => ({ tokens: [{ constraints: [{ attribute: 'is_digit', predicate: { kind: 'flag', value } }], repetition: { kind: 'once' } }] });
  assert.throws(() => plain.add('digit', [flag(true)]), hasCode('SPARS_INVALID_PATTERN'));
  const withModel = new TokenMatcher(model);
  for (const predicate of [{ kind: 'flag', value: 'true' }, { kind: 'flag' }, { kind: 'equals', value: true }, { kind: 'flag', value: true, values: [] }]) {
    assert.throws(() => addMalformed(withModel, 'bad', [untypedItem('is_digit', predicate)]), hasCode('SPARS_INVALID_PATTERN'));
  }
  // A number is a valid predicate value type, so the binding reports a pattern error.
  assert.throws(() => addMalformed(withModel, 'bad', [untypedItem('is_digit', { kind: 'flag', value: 1 })]), hasCode('SPARS_INVALID_PATTERN'));
  // napi rejects a value that is neither a string, a boolean nor a number before conversion.
  assert.throws(() => addMalformed(withModel, 'bad', [untypedItem('is_digit', { kind: 'flag', value: null })]), /none of these types/);
  assert.throws(() => addMalformed(withModel, 'bad', [untypedItem('text', { kind: 'flag', value: true })]), hasCode('SPARS_INVALID_PATTERN'));
  assert.throws(() => Reflect.construct(TokenMatcher, [{}]));
  assert.equal(plain.size, 0);
  assert.equal(withModel.size, 0);
  const falseFlag = flag(false);
  withModel.add('not_digit', [falseFlag]);
  assert.deepStrictEqual(withModel.get('not_digit'), [falseFlag]);
});

test('TokenMatcher remaining lexical flags match the frozen official suite', async () => {
  const model = await loadModel(modelPath);
  const parsed: unknown = JSON.parse(readFileSync(new URL('../../../fixtures/token-match-more-flags-v1.expected.json', import.meta.url), 'utf8'));
  const items = array(record(parsed).cases).map(record);
  assert.equal(items.length, 3);
  let ruleCount = 0;
  let outOfVocabulary = 0;
  let matchCount = 0;
  for (const item of items) {
    const matcher = new TokenMatcher(model);
    const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { ...item, entities: entitiesFromTags(referenceTokens(item.tokens)) } }));
    for (const raw of array(item.rules)) {
      const rule = record(raw);
      outOfVocabulary += addRecorded(matcher, string(rule.name), array(rule.patterns).map(pattern));
      ruleCount++;
    }
    const expected = array(item.expected).map(value => {
      const match = record(value);
      return { rule: string(match.rule), start: number(match.start), end: number(match.end) };
    });
    matchCount += expected.length;
    assert.deepEqual(await matcher.findMatches(doc), expected, string(item.id));
  }
  assert.equal(ruleCount, 99);
  assert.equal(outOfVocabulary, 0);
  assert.equal(matchCount, 1228);
  const plain = new TokenMatcher();
  for (const attribute of ['is_lower', 'is_upper', 'is_title', 'is_ascii', 'is_currency', 'is_stop', 'is_bracket', 'is_quote', 'is_left_punct', 'is_right_punct', 'like_url', 'like_email'] as const) {
    const item: TokenPattern = { tokens: [{ constraints: [{ attribute, predicate: { kind: 'flag', value: false } }], repetition: { kind: 'once' } }] };
    assert.throws(() => plain.add(attribute, [item]), hasCode('SPARS_INVALID_PATTERN'), attribute);
    const withModel = new TokenMatcher(model);
    withModel.add(attribute, [item]);
    assert.deepStrictEqual(withModel.get(attribute), [item]);
  }
  assert.equal(plain.size, 0);
});

test('TokenMatcher LENGTH predicates validate numbers and read back exactly', () => {
  const matcher = new TokenMatcher();
  type LengthPredicate = Extract<TokenConstraint, { attribute: 'length' }>['predicate'];
  const item = (predicate: LengthPredicate): TokenPattern => ({ tokens: [{ constraints: [{ attribute: 'length', predicate }], repetition: { kind: 'once' } }] });
  const valid: LengthPredicate[] = [
    { kind: 'compare', operator: '>', value: 2.5 },
    { kind: 'in_integers', values: [1, -1, 3] },
    { kind: 'not_in_integers', values: [] },
    { kind: 'in_integers', values: [2, 2, -(2 ** 53 - 1), 2 ** 53 - 1] },
  ];
  for (const [index, predicate] of valid.entries()) {
    matcher.add(`length${index}`, [item(predicate)]);
    assert.deepStrictEqual(matcher.get(`length${index}`), [item(predicate)]);
  }
  const invalid = [
    { kind: 'compare', value: 1 },
    { kind: 'compare', operator: '=', value: 1 },
    { kind: 'compare', operator: '==', value: '1' },
    { kind: 'compare', operator: '==', value: Infinity },
    { kind: 'compare', operator: '==', value: Number.NaN },
    { kind: 'compare', operator: '==', value: 1, values: [1] },
    { kind: 'in_integers', values: [1.5] },
    { kind: 'in_integers', values: ['1'] },
    { kind: 'in_integers', values: [2 ** 60] },
    { kind: 'in_integers', values: [2 ** 53] },
    { kind: 'in_integers', values: [-(2 ** 53)] },
    { kind: 'in_integers', value: 1 },
    { kind: 'equals', operator: '==', value: '1' },
    { kind: 'equals', value: '3' },
    { kind: 'in', values: [1] },
  ];
  for (const predicate of invalid) {
    assert.throws(() => addMalformed(matcher, 'bad', [untypedItem('length', predicate)]), hasCode('SPARS_INVALID_PATTERN'), JSON.stringify(predicate));
  }
  assert.throws(() => addMalformed(matcher, 'bad', [untypedItem('text', { kind: 'compare', operator: '==', value: 1 })]), hasCode('SPARS_INVALID_PATTERN'));
  // napi rejects an array mixing numbers and strings before conversion.
  assert.throws(() => addMalformed(matcher, 'bad', [untypedItem('length', { kind: 'in_integers', values: [1, '1'] })]), /none of these types/);
  assert.equal(matcher.contains('bad'), false);
  assert.equal(matcher.size, valid.length);
});

test('TokenMatcher set predicates validate value types and read back exactly', () => {
  const matcher = new TokenMatcher();
  const item = (condition: TokenConstraint): TokenPattern => ({ tokens: [{ constraints: [condition], repetition: { kind: 'once' } }] });
  const valid: TokenConstraint[] = [
    { attribute: 'lower', predicate: { kind: 'is_subset', values: ['the', 'a'] } },
    { attribute: 'pos', predicate: { kind: 'is_superset', values: [] } },
    { attribute: 'morphology', predicate: { kind: 'intersects', values: ['Number=Plur'] } },
    { attribute: 'morphology', predicate: { kind: 'is_subset', values: ['Number=Plur', 'Person=3'] } },
    { attribute: 'length', predicate: { kind: 'is_subset_integers', values: [1, 2] } },
    { attribute: 'length', predicate: { kind: 'is_superset_integers', values: [2 ** 53 - 1] } },
    { attribute: 'length', predicate: { kind: 'intersects_integers', values: [-1, 3] } },
  ];
  for (const [index, condition] of valid.entries()) {
    matcher.add(`set${index}`, [item(condition)]);
    assert.deepStrictEqual(matcher.get(`set${index}`), [item(condition)]);
  }
  const invalid = [
    ['length', { kind: 'is_subset', values: ['1'] }],
    ['lower', { kind: 'is_subset', values: [1] }],
    ['lower', { kind: 'is_subset_integers', values: [1] }],
    ['is_alpha', { kind: 'intersects', values: [] }],
    ['length', { kind: 'is_superset_integers', values: [1.5] }],
    ['length', { kind: 'intersects_integers', values: [2 ** 53] }],
    ['length', { kind: 'intersects_integers', value: 1 }],
    ['morphology', { kind: 'is_subset', values: ['Number'] }],
    ['lower', { kind: 'morph_superset', values: ['a'] }],
    ['lower', { kind: 'IS_SUBSET', values: ['a'] }],
  ];
  for (const [attribute, predicate] of invalid) {
    assert.throws(() => addMalformed(matcher, 'bad', [untypedItem(attribute, predicate)]), hasCode('SPARS_INVALID_PATTERN'), JSON.stringify([attribute, predicate]));
  }
  // napi rejects an array mixing strings and numbers before conversion.
  assert.throws(() => addMalformed(matcher, 'bad', [untypedItem('lower', { kind: 'is_subset', values: ['a', 1] })]), /none of these types/);
  assert.equal(matcher.contains('bad'), false);
  assert.equal(matcher.size, valid.length);
});

test('TokenMatcher lifecycle, ownership, and pending mutation', async () => {
  const matcher = new TokenMatcher();
  const input = plainDoc('xx');
  const two: TokenPattern = { tokens: [...wildcard().tokens, ...wildcard().tokens] };
  matcher.add('a', [two]);
  matcher.add('b', [wildcard()]);
  matcher.add('a', [wildcard(), wildcard()]);
  assert.equal(matcher.size, 2);
  assert.equal(matcher.contains('a'), true);
  assert.equal(matcher.get('a')?.length, 3);
  two.tokens.splice(0);
  matcher.get('a')?.splice(0);
  assert.equal(matcher.get('a')?.length, 3);
  const pending = matcher.findMatches(input);
  assert.throws(() => matcher.remove('a'), hasCode('SPARS_BUSY'));
  assert.throws(() => matcher.add('c', []), hasCode('SPARS_BUSY'));
  assert.deepEqual(await pending, [
    { rule: 'b', start: 0, end: 1 }, { rule: 'a', start: 0, end: 1 },
    { rule: 'a', start: 0, end: 2 }, { rule: 'b', start: 1, end: 2 }, { rule: 'a', start: 1, end: 2 },
  ]);
  matcher.remove('a');
  assert.equal(matcher.get('a'), null);
  assert.equal(matcher.contains('a'), false);
  assert.throws(() => matcher.remove('a'), hasCode('SPARS_INVALID_PATTERN'));
  assert.deepEqual(await matcher.findMatches(plainDoc('x')), [{ rule: 'b', start: 0, end: 1 }]);
});

test('TokenMatcher rejects malformed patterns atomically and reports unavailable annotations', async () => {
  const matcher = new TokenMatcher();
  const repeated = (repetition: unknown): unknown => ({ tokens: [{ constraints: [], repetition }] });
  const invalid: unknown[] = [
    { tokens: [] }, repeated({ kind: 'unknown' }), repeated({ kind: 'once', min: 0 }),
    repeated({ kind: 'range' }), wildcard({ kind: 'range', min: 2, max: 1 }),
    ...[-1, 0.5, NaN, Infinity, 4294967296].map(min => wildcard({ kind: 'range', min })),
    wildcard({ kind: 'range', min: 4097 }),
    ...[
      { attribute: 'unknown', predicate: { kind: 'equals', value: 'a' } },
      { attribute: 'LOWER', predicate: { kind: 'equals', value: 'a' } },
      { attribute: 'lower', predicate: { kind: 'morph_superset', values: ['a'] } },
      { attribute: 'text', predicate: { kind: 'unknown', values: [] } },
      { attribute: 'text', predicate: { kind: 'equals' } },
      { attribute: 'text', predicate: { kind: 'equals', value: 'a', values: [] } },
      { attribute: 'text', predicate: { kind: 'in', value: 'a', values: [] } },
      { attribute: 'text', predicate: { kind: 'in' } },
      { attribute: 'text', predicate: { kind: 'morph_superset', values: ['Case=Nom'] } },
      { attribute: 'morphology', predicate: { kind: 'morph_superset', values: ['invalid'] } },
    ].map(constraint => ({ tokens: [{ constraints: [constraint], repetition: { kind: 'once' } }] })),
  ];
  for (const pattern of invalid) {
    assert.throws(() => addMalformed(matcher, 'invalid', [wildcard(), pattern]), hasCode('SPARS_INVALID_PATTERN'));
    assert.equal(matcher.size, 0);
  }
  for (const call of [() => matcher.contains('\ud800'), () => matcher.get('\ud800'), () => matcher.remove('\ud800'), () => matcher.add('\ud800', [])]) {
    assert.throws(call, hasCode('SPARS_INVALID_TEXT'));
  }
  assert.throws(() => matcher.add('bad', [{ tokens: [{ constraints: [{ attribute: 'text', predicate: { kind: 'equals', value: '\ud800' } }], repetition: { kind: 'once' } }] }]), hasCode('SPARS_INVALID_TEXT'));
  matcher.add('lemma', [{ tokens: [{ constraints: [{ attribute: 'lemma', predicate: { kind: 'equals', value: 'x' } }], repetition: { kind: 'once' } }] }]);
  await assert.rejects(matcher.findMatches(plainDoc('x')), (error: unknown) => hasCode('SPARS_INFERENCE')(error) && error instanceof Error && /lemma/i.test(error.message));
});
test('partly annotated snapshots compare missing values like spaCy', async () => {
  // Expected spans are spaCy 3.8.14 Matcher results: E155 only when no token has an annotation
  // an equality condition compares; a missing lemma compares as '', and missing morphology
  // (null) differs from the empty analysis ('').
  const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text: 'abc', tokens: [
    { start: 0, end: 1, idx: 0, norm: 'a', whitespace: false, lemma: 'x', morphology: 'Number=Plur' },
    { start: 1, end: 2, idx: 1, norm: 'b', whitespace: false, morphology: '' },
    { start: 2, end: 3, idx: 2, norm: 'c', whitespace: false },
  ] } }));
  const spans = async (constraint: TokenConstraint): Promise<Array<[number, number]>> => {
    const matcher = new TokenMatcher();
    matcher.add('r', [{ tokens: [{ constraints: [constraint], repetition: { kind: 'once' } }] }]);
    return (await matcher.findMatches(doc)).map(m => [m.start, m.end]);
  };
  assert.deepEqual(await spans({ attribute: 'lemma', predicate: { kind: 'equals', value: 'x' } }), [[0, 1]]);
  assert.deepEqual(await spans({ attribute: 'lemma', predicate: { kind: 'not_in', values: ['x'] } }), [[1, 2], [2, 3]]);
  assert.deepEqual(await spans({ attribute: 'morphology', predicate: { kind: 'equals', value: '_' } }), [[1, 2]]);
  assert.deepEqual(await spans({ attribute: 'morphology', predicate: { kind: 'equals', value: '' } }), [[2, 3]]);
  assert.deepEqual(await spans({ attribute: 'morphology', predicate: { kind: 'in', values: [''] } }), [[1, 2]]);
  // Membership needs no annotated token; equality does.
  assert.deepEqual(await spans({ attribute: 'pos', predicate: { kind: 'not_in', values: ['NOUN'] } }), [[0, 1], [1, 2], [2, 3]]);
  await assert.rejects(spans({ attribute: 'pos', predicate: { kind: 'equals', value: 'NOUN' } }), hasCode('SPARS_INFERENCE'));
});
