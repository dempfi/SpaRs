import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DependencyMatcher, loadModel, NativeDocument } from '../index.js';
import type { DependencyPattern, DependencyNode } from '../index.js';
import { array, entitiesFromTags, modelPath, readJson, record, referenceTokens, root } from './fixtures.mts';
import { recordedConstraint, relation } from './vocabulary.mts';
import type { Recorded } from './vocabulary.mts';

function string(value: unknown): string { assert.ok(typeof value === 'string'); return value; }
function pattern(value: unknown): Recorded<DependencyPattern> {
  const nodes = array(record(value).nodes).map(value => {
    const n = record(value);
    const link = n.link === null || n.link === undefined ? undefined : record(n.link);
    return {
      id: string(n.id),
      constraints: array(n.constraints).map(recordedConstraint),
      ...(link === undefined ? {} : { link: { left: string(link.left), relation: relation(link.relation) } }),
    };
  });
  const typed: DependencyNode[] = [];
  for (const node of nodes) {
    const constraints = node.constraints.flatMap(c => c.typed ? [c.constraint] : []);
    if (constraints.length !== node.constraints.length) {
      return { typed: false, pattern: { nodes: nodes.map(n => ({ ...n, constraints: n.constraints.map(c => c.constraint) })) } };
    }
    typed.push({ ...node, constraints });
  }
  return { typed: true, pattern: { nodes: typed } };
}
/** Add recorded patterns, using an untyped call only for values the declarations deliberately exclude. */
function addRecorded(matcher: DependencyMatcher, rule: string, patterns: Recorded<DependencyPattern>[]): number {
  const typed = patterns.flatMap(p => p.typed ? [p.pattern] : []);
  if (typed.length === patterns.length) {
    matcher.add(rule, typed);
    return 0;
  }
  Reflect.apply(matcher.add, matcher, [rule, patterns.map(p => p.pattern)]);
  return 1;
}
function document(value: unknown): NativeDocument {
  const c = record(value);
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: {
    text: c.text, tokens: c.tokens, sentences: c.sentences, entities: entitiesFromTags(referenceTokens(c.tokens)), noun_chunks: c.noun_chunks,
  } }));
}
const singleton: DependencyPattern = { nodes: [{ id: 'a', constraints: [] }] };
function tiny(): NativeDocument {
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: {
    text: 'a b', tokens: [
      { start: 0, end: 1, idx: 0, whitespace: true, norm: 'a', head: 1, dep: 'dep' },
      { start: 2, end: 3, idx: 2, whitespace: false, norm: 'b', head: 1, dep: 'ROOT' },
    ], sentences: null, entities: null, noun_chunks: null,
  } }));
}

test('all dependency relations, predicates and ordering match frozen official suites', async () => {
  const relations = new Set<string>();
  let lowerRules = 0;
  let lowerMatches = 0;
  let lengthRules = 0;
  let lengthMatches = 0;
  let setRules = 0;
  let setMatches = 0;
  let setUntyped = 0;
  let normalizationRules = 0;
  let normalizationMatches = 0;
  for (const [suite, count] of [['dependency-match-v1', 6], ['dependency-match-regressions-v1', 2], ['dependency-match-lower-v1', 4], ['dependency-match-length-v1', 3], ['dependency-match-sets-v1', 3], ['dependency-match-morph-normalization-v1', 1]] as const) {
    const fixture = record(readJson(`${root}fixtures/${suite}.expected.json`));
    assert.deepEqual(fixture.versions, { spacy: '3.8.14', thinc: '8.3.13' });
    const cases = array(fixture.cases);
    assert.equal(cases.length, count);
    for (const value of cases) {
      const c = record(value);
      const matcher = new DependencyMatcher();
      const rules = array(c.rules);
      const registered = new Map<string, unknown[]>();
      if (suite === 'dependency-match-v1') assert.equal(rules.length, 52);
      if (suite === 'dependency-match-lower-v1') {
        lowerRules += rules.length;
        lowerMatches += array(c.expected).length;
      }
      if (suite === 'dependency-match-length-v1') {
        lengthRules += rules.length;
        lengthMatches += array(c.expected).length;
      }
      if (suite === 'dependency-match-sets-v1') {
        setRules += rules.length;
        setMatches += array(c.expected).length;
      }
      if (suite === 'dependency-match-morph-normalization-v1') {
        normalizationRules += rules.length;
        normalizationMatches += array(c.expected).length;
      }
      for (const value of rules) {
        const rule = record(value);
        const patterns = array(rule.patterns).map(pattern);
        for (const raw of array(rule.patterns)) {
          for (const node of array(record(raw).nodes).map(record)) if (node.link !== null && node.link !== undefined) relations.add(relation(record(node.link).relation));
        }
        const name = string(rule.name);
        const untyped = addRecorded(matcher, name, patterns);
        if (suite === 'dependency-match-sets-v1') setUntyped += untyped;
        else assert.equal(untyped, 0);
        const expectedPatterns = [...(registered.get(name) ?? []), ...patterns.map(p => p.pattern)];
        registered.set(name, expectedPatterns);
        const actualPatterns = matcher.get(name);
        assert.ok(actualPatterns);
        assert.deepEqual(actualPatterns.map(p => pattern(p).pattern), expectedPatterns);
      }
      if (suite === 'dependency-match-v1') assert.equal(matcher.size, 51);
      const doc = document(c);
      assert.deepEqual(await matcher.findMatches(doc), c.expected, string(c.id));
      assert.deepEqual(await matcher.findMatches(doc), c.expected);
      const concurrent = await Promise.all([matcher.findMatches(doc), matcher.findMatches(doc)]);
      assert.deepEqual(concurrent, [c.expected, c.expected]);
    }
  }
  assert.equal(relations.size, 20);
  assert.equal(lowerRules, 160);
  assert.equal(lowerMatches, 223);
  assert.equal(lengthRules, 96);
  assert.equal(lengthMatches, 321);
  assert.equal(setRules, 546);
  assert.equal(setMatches, 587);
  // Rules with POS values outside the universal tags; see vocabulary.mts.
  assert.equal(setUntyped, 9);
  assert.equal(normalizationRules, 66);
  assert.equal(normalizationMatches, 80);
});

const FLAG_RULES = 42;
const FLAG_MATCHES = 376;
test('lexical flags match the frozen official suite with a model lexicon', async () => {
  const model = await loadModel(modelPath);
  const fixture = record(readJson(`${root}fixtures/dependency-match-flags-v1.expected.json`));
  const cases = array(fixture.cases);
  assert.equal(cases.length, 3);
  let rules = 0;
  let matches = 0;
  for (const value of cases) {
    const c = record(value);
    const matcher = new DependencyMatcher(model);
    for (const raw of array(c.rules)) {
      const rule = record(raw);
      assert.equal(addRecorded(matcher, string(rule.name), array(rule.patterns).map(pattern)), 0);
      rules++;
    }
    matches += array(c.expected).length;
    const doc = document(c);
    assert.deepEqual(await matcher.findMatches(doc), c.expected, string(c.id));
  }
  assert.equal(rules, FLAG_RULES);
  assert.equal(matches, FLAG_MATCHES);
  const flag: DependencyPattern = { nodes: [{ id: 'a', constraints: [{ attribute: 'like_num', predicate: { kind: 'flag', value: true } }] }] };
  const plain = new DependencyMatcher();
  assert.throws(() => plain.add('number', [flag]), (error: unknown) => error instanceof Error && 'code' in error && error.code === 'SPARS_INVALID_PATTERN');
  assert.equal(plain.size, 0);
  const withModel = new DependencyMatcher(model);
  withModel.add('number', [flag]);
  assert.deepStrictEqual(withModel.get('number'), [flag]);
  assert.throws(() => Reflect.construct(DependencyMatcher, [{}]));
});

test('remaining lexical flags match the frozen official suite', async () => {
  const model = await loadModel(modelPath);
  const fixture = record(readJson(`${root}fixtures/dependency-match-more-flags-v1.expected.json`));
  const cases = array(fixture.cases);
  assert.equal(cases.length, 3);
  let rules = 0;
  let matches = 0;
  for (const value of cases) {
    const c = record(value);
    const matcher = new DependencyMatcher(model);
    for (const raw of array(c.rules)) {
      const rule = record(raw);
      assert.equal(addRecorded(matcher, string(rule.name), array(rule.patterns).map(pattern)), 0);
      rules++;
    }
    matches += array(c.expected).length;
    assert.deepEqual(await matcher.findMatches(document(c)), c.expected, string(c.id));
  }
  assert.equal(rules, 90);
  assert.equal(matches, 1209);
});

test('rule lifecycle appends patterns and isolates returned objects', async () => {
  const matcher = new DependencyMatcher();
  const doc = tiny();
  assert.equal(matcher.size, 0);
  assert.equal(matcher.get('absent'), null);
  assert.equal(matcher.contains('absent'), false);
  assert.deepEqual(await matcher.findMatches(doc), []);
  matcher.add('rule', [singleton]);
  assert.equal(matcher.contains('rule'), true);
  const saved = matcher.get('rule');
  assert.ok(saved);
  assert.equal(saved.length, 1);
  const first = saved[0];
  assert.ok(first);
  first.nodes.length = 0;
  assert.equal(matcher.get('rule')?.[0]?.nodes.length, 1);
  matcher.add('rule', [singleton]);
  assert.equal(matcher.size, 1);
  assert.equal((await matcher.findMatches(doc)).length, 4);
  matcher.remove('rule');
  assert.equal(matcher.size, 0);
  assert.throws(() => matcher.remove('rule'));
});

test('invalid patterns and predicates fail atomically', () => {
  const matcher = new DependencyMatcher();
  matcher.add('good', [singleton]);
  // Deliberately malformed, so untyped; each is passed through Reflect.apply.
  const invalid: unknown[] = [
    { nodes: [] },
    { nodes: [{ id: 'a', constraints: [], link: { left: 'a', relation: '>' } }] },
    { nodes: [{ id: 'a', constraints: [] }, { id: 'b', constraints: [] }] },
    { nodes: [{ id: 'a', constraints: [] }, { id: 'a', constraints: [], link: { left: 'a', relation: '>' } }] },
    { nodes: [{ id: 'a', constraints: [] }, { id: 'b', constraints: [], link: { left: 'missing', relation: '>' } }] },
    { nodes: [{ id: 'a', constraints: [] }, { id: 'b', constraints: [], link: { left: 'a', relation: 'unknown' } }] },
    { nodes: [{ id: 'a', constraints: [{ attribute: 'unknown', predicate: { kind: 'equals', value: 'a' } }] }] },
    { nodes: [{ id: 'a', constraints: [{ attribute: 'LOWER', predicate: { kind: 'equals', value: 'a' } }] }] },
    { nodes: [{ id: 'a', constraints: [{ attribute: 'lower', predicate: { kind: 'morph_superset', values: ['a'] } }] }] },
    { nodes: [{ id: 'a', constraints: [{ attribute: 'text', predicate: { kind: 'regex', value: 'a' } }] }] },
    { nodes: [{ id: 'a', constraints: [{ attribute: 'text', predicate: { kind: 'equals' } }] }] },
    { nodes: [{ id: 'a', constraints: [{ attribute: 'text', predicate: { kind: 'equals', value: 'a', values: ['a'] } }] }] },
  ];
  for (const p of invalid) {
    assert.throws(() => Reflect.apply(matcher.add, matcher, ['bad', [singleton, p]]));
    assert.equal(matcher.contains('bad'), false);
    assert.throws(() => Reflect.apply(matcher.add, matcher, ['good', [singleton, p]]));
    assert.equal(matcher.get('good')?.length, 1);
  }
  assert.throws(() => matcher.add('\ud800', [singleton]), { code: 'SPARS_INVALID_TEXT' });
  assert.throws(() => matcher.add('bad', [{ nodes: [{ id: '\ud800', constraints: [] }] }]), { code: 'SPARS_INVALID_TEXT' });
});

test('pending searches retain documents and block rule mutation until resolution', async () => {
  const matcher = new DependencyMatcher();
  matcher.add('rule', [singleton]);
  const pending = matcher.findMatches(tiny());
  assert.throws(() => matcher.add('next', [singleton]), { code: 'SPARS_BUSY' });
  assert.throws(() => matcher.remove('rule'), { code: 'SPARS_BUSY' });
  global.gc?.();
  assert.deepEqual(await pending, [{ rule: 'rule', tokens: [0] }, { rule: 'rule', tokens: [1] }]);
  matcher.remove('rule');
});

test('noncanonical document morphology rejects asynchronously', async () => {
  const doc = (morphology: string): NativeDocument => NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: {
    text: 'a', tokens: [{ start: 0, end: 1, idx: 0, whitespace: false, norm: 'a', head: 0, dep: 'ROOT', morphology }],
    sentences: null, entities: null, noun_chunks: null,
  } }));
  const matcher = new DependencyMatcher();
  matcher.add('rule', [{ nodes: [{ id: 'a', constraints: [{ attribute: 'morphology', predicate: { kind: 'intersects', values: ['Case=Nom'] } }] }] }]);
  await assert.rejects(matcher.findMatches(doc('Case=Nom|Case2=Acc')), { code: 'SPARS_UNSUPPORTED' });
  assert.deepEqual(await matcher.findMatches(doc('Case2=Acc|Case=Nom')), [{ rule: 'rule', tokens: [0] }]);
});

test('missing dependency heads and unavailable constrained annotations reject asynchronously', async () => {
  const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: {
    text: 'a', tokens: [{ start: 0, end: 1, idx: 0, whitespace: false, norm: 'a' }],
    sentences: null, entities: null, noun_chunks: null,
  } }));
  const matcher = new DependencyMatcher();
  matcher.add('rule', [singleton]);
  await assert.rejects(matcher.findMatches(doc));
  matcher.remove('rule');
  matcher.add('lemma', [{ nodes: [{ id: 'a', constraints: [{ attribute: 'lemma', predicate: { kind: 'equals', value: 'a' } }] }] }]);
  await assert.rejects(matcher.findMatches(tiny()));
});
