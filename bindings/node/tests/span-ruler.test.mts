import assert from 'node:assert/strict';
import { test } from 'node:test';
import { configureExecution, configureInputLimits, loadModel, NativeDocument, SpanRuler } from '../index.js';
import type { SpanPattern, SpanRulerOptions, SpanRulerPattern, TokenPattern } from '../index.js';
import { array, modelPath, readJson, record, root } from './fixtures.mts';
import { member, PHRASE_ATTRIBUTES, recordedConstraint, repetition, SPAN_FILTERS } from './vocabulary.mts';

function string(value: unknown): string { assert.ok(typeof value === 'string'); return value; }
function nullableString(value: unknown): string | null { return value === null ? null : string(value); }
function integer(value: unknown): number { assert.ok(typeof value === 'number' && Number.isSafeInteger(value)); return value; }
function boolean(value: unknown): boolean { assert.ok(typeof value === 'boolean'); return value; }
function hasCode(code: string): (error: unknown) => boolean {
  return (error: unknown): boolean => error instanceof Error && 'code' in error && error.code === code;
}

/** Parse a recorded token pattern; every recorded condition must fit the declared types. */
function tokenPattern(value: unknown): TokenPattern {
  return { tokens: array(record(value).tokens).map(value => {
    const item = record(value);
    return { constraints: array(item.constraints).map(value => {
      const parsed = recordedConstraint(value);
      assert.ok(parsed.typed);
      return parsed.constraint;
    }), repetition: repetition(item.repetition) };
  }) };
}

/** Restore a recorded document; format 4 carries entity IDs and span groups. */
function snapshot(value: unknown): NativeDocument {
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 4, document: record(value) }));
}

/** The recorded ID, leaving the field out for none, as a caller would; `''` is kept. */
function withId<T extends object>(base: T, id: string | null): T & { id?: string } {
  return id === null ? base : { ...base, id };
}

function pattern(value: unknown): SpanPattern {
  const p = record(value);
  const base = withId({ label: string(p.label) }, nullableString(p.id));
  if (p.tokens !== null) {
    assert.equal(p.phrase, null);
    return { ...base, tokens: tokenPattern(p.tokens) };
  }
  return { ...base, phrase: snapshot(p.phrase) };
}

function listed(value: unknown): SpanRulerPattern {
  const p = record(value);
  const base = withId({ label: string(p.label) }, nullableString(p.id));
  if (p.tokens !== null) {
    assert.equal(p.phrase, null);
    return { ...base, tokens: tokenPattern(p.tokens) };
  }
  return { ...base, phrase: string(p.phrase) };
}

/** A match as plain numbers; `deepEqual` compares the branded indices by value. */
function matched(value: unknown): { label: string; id: string; start: number; end: number } {
  const m = record(value);
  // Node reports a missing ID as '', as it does on span group members.
  return { start: integer(m.start), end: integer(m.end), label: string(m.label), id: m.id == null ? '' : string(m.id) };
}

function filter(value: unknown): (typeof SPAN_FILTERS)[number] { return member(SPAN_FILTERS, value); }

/** spaCy's config names as Node options; `spans_key: None` is `spansKey: null`. */
function options(value: unknown): SpanRulerOptions {
  const o = record(value);
  return {
    spansKey: nullableString(o.spans_key),
    spansFilter: o.spans_filter === null ? null : filter(o.spans_filter),
    annotateEntities: boolean(o.annotate_entities),
    entitiesFilter: filter(o.entities_filter),
    phraseAttribute: member(PHRASE_ATTRIBUTES, o.phrase_attribute),
    overwrite: boolean(o.overwrite),
  };
}

function state(ruler: SpanRuler): unknown {
  return { size: ruler.size, labels: ruler.labels, ids: ruler.ids, patterns: ruler.patterns };
}

test('SpanRuler replays every frozen spaCy outcome', async () => {
  const fixture = record(readJson(`${root}fixtures/span-ruler-v1.expected.json`));
  assert.equal(fixture.format_version, 1);
  assert.deepEqual(fixture.versions, { spacy: '3.8.14', en_core_web_md: '3.8.0' });
  let [cases, steps, applications, matches, errors] = [0, 0, 0, 0, 0];
  for (const value of array(fixture.cases)) {
    const c = record(value);
    cases += 1;
    const settings = options(c.options);
    const ruler = new SpanRuler(settings);
    assert.deepEqual([ruler.spansKey, ruler.spansFilter, ruler.annotateEntities, ruler.entitiesFilter, ruler.phraseAttribute, ruler.overwrite],
      [settings.spansKey, settings.spansFilter, settings.annotateEntities, settings.entitiesFilter, settings.phraseAttribute, settings.overwrite]);
    const probes = array(c.probes).map(string);
    const initial = snapshot(c.document);
    let previous = initial;
    const outcomes = array(c.outcomes).map(record);
    assert.equal(outcomes.length, array(c.steps).length);
    for (const [index, stepValue] of array(c.steps).entries()) {
      const step = record(stepValue);
      const outcome = outcomes[index];
      assert.ok(outcome !== undefined);
      const at = `${string(c.id)} step ${index}`;
      steps += 1;
      let error: string | null = null;
      const removed = (remove: () => void): void => {
        try {
          remove();
        } catch (caught) {
          assert.ok(hasCode('SPARS_INVALID_PATTERN')(caught), at);
          error = 'ValueError';
        }
      };
      switch (step.action) {
        case 'add':
          ruler.add(array(step.patterns).map(pattern));
          break;
        case 'remove':
          removed(() => ruler.remove(string(step.argument)));
          break;
        case 'remove_by_id':
          removed(() => ruler.removeById(string(step.argument)));
          break;
        case 'clear':
          ruler.clear();
          break;
        case 'apply': {
          applications += 1;
          assert.ok(step.input === 'initial' || step.input === 'previous', at);
          const input = step.input === 'initial' ? initial : previous;
          const before = input.toSnapshot();
          const expected = array(outcome.matches).map(matched);
          matches += expected.length;
          assert.deepEqual(await ruler.findMatches(input), expected, at);
          const annotated = await ruler.annotate(input);
          assert.deepEqual(annotated.toObject(), snapshot(outcome.result).toObject(), at);
          // Snapshots keep the ruler's span groups, entities and IDs, and the input is unchanged.
          assert.deepEqual(NativeDocument.fromSnapshot(annotated.toSnapshot()).toObject(), annotated.toObject(), at);
          assert.equal(input.toSnapshot(), before, at);
          previous = annotated;
          break;
        }
        default:
          assert.fail(`${at}: unknown action`);
      }
      errors += error === null ? 0 : 1;
      assert.equal(error, nullableString(outcome.error), at);
      assert.deepEqual(state(ruler), { size: integer(outcome.length), labels: array(outcome.labels).map(string),
        ids: array(outcome.ids).map(string), patterns: array(outcome.patterns).map(listed) }, at);
      assert.deepEqual(probes.filter(label => ruler.contains(label)), array(outcome.contained).map(string), at);
    }
  }
  assert.deepEqual([cases, steps, applications, matches, errors], [100, 378, 188, 799, 15]);
});

/** A tokenizer-only document with one token per word. */
function doc(words: string[]): NativeDocument {
  let text = '';
  const tokens = words.map((word, i) => {
    const start = Buffer.byteLength(text);
    const idx = [...text].length;
    text += word;
    const whitespace = i + 1 < words.length;
    if (whitespace) text += ' ';
    return { start, end: start + Buffer.byteLength(word), idx, whitespace, norm: word.toLowerCase() };
  });
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text, tokens } }));
}
const acme: TokenPattern = { tokens: [{ constraints: [{ attribute: 'lower', predicate: { kind: 'equals', value: 'acme' } }], repetition: { kind: 'once' } }] };

function settings(ruler: SpanRuler): unknown[] {
  return [ruler.spansKey, ruler.spansFilter, ruler.annotateEntities, ruler.entitiesFilter, ruler.phraseAttribute, ruler.overwrite];
}

test('SpanRuler options default like spaCy and reject invalid settings', () => {
  const defaults = ['ruler', null, false, 'first_longest', 'ORTH', true];
  assert.deepEqual([...settings(new SpanRuler()), new SpanRuler().size], [...defaults, 0]);
  for (const options of [{}, null, undefined, { spansFilter: null, annotateEntities: null, entitiesFilter: null, phraseAttribute: null, overwrite: null }]) {
    assert.deepEqual(settings(new SpanRuler(options)), defaults, JSON.stringify(options));
  }
  // Only `null` disables the span group; an omitted or undefined key keeps the default. The
  // declarations reject an explicit undefined, which untyped JavaScript can still pass.
  assert.equal(Reflect.construct(SpanRuler, [{ spansKey: undefined }]).spansKey, 'ruler');
  assert.equal(new SpanRuler({ spansKey: null }).spansKey, null);
  assert.equal(new SpanRuler({ spansKey: '' }).spansKey, '');
  for (const name of SPAN_FILTERS) {
    const ruler = new SpanRuler({ spansFilter: name, entitiesFilter: name });
    assert.deepEqual([ruler.spansFilter, ruler.entitiesFilter], [name, name]);
  }
  const custom = new SpanRuler({ spansKey: 'sc', annotateEntities: true, phraseAttribute: 'LOWER', overwrite: false });
  assert.deepEqual(settings(custom), ['sc', null, true, 'first_longest', 'LOWER', false]);
  for (const options of [{ spansFilter: 'prioritizeNew' }, { entitiesFilter: 'spacy.first_longest_spans_filter.v1' }, { entitiesFilter: 'FIRST_LONGEST' },
    { phraseAttribute: 'SHAPE' }, { phraseAttribute: 'lower' }]) {
    assert.throws(() => Reflect.construct(SpanRuler, [options]), hasCode('SPARS_UNSUPPORTED'), JSON.stringify(options));
  }
  assert.deepEqual(settings(Reflect.construct(SpanRuler, [{ spansFilter: undefined, entitiesFilter: undefined, overwrite: undefined }, null])), defaults);
  for (const name of PHRASE_ATTRIBUTES) assert.equal(new SpanRuler({ phraseAttribute: name }).phraseAttribute, name);
  for (const name of ['spansKey', 'spansFilter', 'entitiesFilter', 'phraseAttribute']) {
    assert.throws(() => Reflect.construct(SpanRuler, [{ [name]: '\uD800' }]), hasCode('SPARS_INVALID_TEXT'), name);
  }
  // Unknown options, such as spaCy's own names, values of the wrong type and arrays are rejected.
  for (const options of [{ spans_key: 'x' }, { annotateEnts: true }, { spansKey: 1 }, { spansKey: true }, { spansFilter: 1 }, { entitiesFilter: 1 },
    { phraseAttribute: 1 }, { overwrite: 'yes' }, { annotateEntities: 1 }, [], ['x']]) {
    assert.throws(() => Reflect.construct(SpanRuler, [options]), hasCode('InvalidArg'), JSON.stringify(options));
  }
  // Lexical flag phrases need a model; without one every phrase addition fails.
  const flags = new SpanRuler({ phraseAttribute: 'IS_ALPHA' });
  assert.throws(() => flags.add([{ label: 'X', phrase: doc(['a']) }]), hasCode('SPARS_INVALID_PATTERN'));
});

test('SpanRuler rejects malformed patterns without changing its rules', async () => {
  const ruler = new SpanRuler();
  ruler.add([{ label: 'ORG', id: 'acme', tokens: acme }]);
  const before = state(ruler);
  const valid: SpanPattern = { label: 'NEW', phrase: doc(['new']) };
  for (const malformed of [
    { label: 'X', pattern: 'x' },
    { label: 'X' },
    { label: 'X', tokens: acme, phrase: doc(['x']) },
    { label: 'X', phrase: { text: 'x' } },
    { label: 'X', tokens: { tokens: [{ constraints: [], repetition: { kind: 'once' }, extra: 1 }] } },
    { label: 'X', tokens: { tokens: [] } },
    { tokens: acme },
    { label: 1, tokens: acme },
    { label: 'X', id: 1, tokens: acme },
  ]) {
    assert.throws(() => Reflect.apply(ruler.add, ruler, [[valid, malformed]]), hasCode('SPARS_INVALID_PATTERN'), JSON.stringify(Object.keys(malformed)));
    assert.deepEqual(state(ruler), before);
  }
  for (const patterns of ['x', [null], ['x']]) {
    assert.throws(() => Reflect.apply(ruler.add, ruler, [patterns]), hasCode('SPARS_INVALID_PATTERN'), JSON.stringify(patterns));
  }
  ruler.add([]);
  assert.deepEqual(state(ruler), before);
  // Changing a listing does not change the ruler.
  const listing = ruler.patterns;
  listing.pop();
  assert.deepEqual(state(ruler), before);
  // A phrase without the annotation the phrase attribute compares is an invalid pattern.
  const lemmas = new SpanRuler({ phraseAttribute: 'LEMMA' });
  assert.throws(() => lemmas.add([{ label: 'X', tokens: acme }, { label: 'X', phrase: doc(['a']) }]), hasCode('SPARS_INVALID_PATTERN'));
  assert.deepEqual(state(lemmas), { size: 0, labels: [], ids: [], patterns: [] });
  assert.equal(lemmas.contains('X'), false);
  assert.throws(() => ruler.add([{ label: '\uD800', tokens: acme }]), hasCode('SPARS_INVALID_TEXT'));
  assert.throws(() => ruler.add([{ label: 'X', id: '\uD800', tokens: acme }]), hasCode('SPARS_INVALID_TEXT'));
  // Morphology phrases must be canonical (fields in spaCy's order).
  const morphology = new SpanRuler({ phraseAttribute: 'MORPH' });
  const unordered = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text: 'I',
    tokens: [{ start: 0, end: 1, idx: 0, whitespace: false, norm: 'i', morphology: 'Number=Sing|Case=Nom' }] } }));
  assert.throws(() => morphology.add([{ label: 'X', phrase: unordered }]), hasCode('SPARS_UNSUPPORTED'));
  assert.equal(morphology.size, 0);
  for (const method of ['remove', 'removeById', 'contains'] as const) {
    assert.throws(() => ruler[method]('\uD800'), hasCode('SPARS_INVALID_TEXT'), method);
  }
  assert.deepEqual(state(ruler), before);
  // An untyped null ID or tokens field counts as absent, not as ''.
  Reflect.apply(ruler.add, ruler, [[{ label: 'NULL', id: null, tokens: acme, phrase: null }]]);
  assert.deepEqual([ruler.patterns.at(-1), ruler.ids], [{ label: 'NULL', tokens: acme }, ['acme']]);
  ruler.remove('NULL');
  assert.throws(() => ruler.remove('missing'), hasCode('SPARS_INVALID_PATTERN'));
  assert.throws(() => ruler.removeById('missing'), hasCode('SPARS_INVALID_PATTERN'));
  ruler.removeById('acme');
  assert.throws(() => ruler.removeById('acme'), hasCode('SPARS_INVALID_PATTERN'));
  // The label stays known, so removing it again succeeds.
  assert.equal(ruler.contains('ORG'), true);
  ruler.remove('ORG');
  assert.deepEqual(await ruler.findMatches(doc(['Acme'])), []);
});

test('SpanRuler annotates copies and rejects rule changes while searches are pending', async () => {
  const ruler = new SpanRuler({ annotateEntities: true });
  ruler.add([{ label: 'ORG', id: 'acme', tokens: acme }, { label: 'ORG', id: '', phrase: doc(['Acme', 'Corp']) }]);
  assert.deepEqual(ruler.patterns, [{ label: 'ORG', id: 'acme', tokens: acme }, { label: 'ORG', id: '', phrase: 'Acme Corp' }]);
  assert.deepEqual(ruler.ids, ['', 'acme']);
  const input = doc(['Acme', 'Corp', 'rocks']);
  // Start each search alone, so the checks do not depend on which one finishes first.
  for (const start of [() => ruler.findMatches(input), () => ruler.annotate(input)]) {
    const pending = start();
    assert.throws(() => ruler.add([{ label: 'X', tokens: acme }]), hasCode('SPARS_BUSY'));
    assert.throws(() => ruler.remove('ORG'), hasCode('SPARS_BUSY'));
    assert.throws(() => ruler.removeById('acme'), hasCode('SPARS_BUSY'));
    assert.throws(() => ruler.clear(), hasCode('SPARS_BUSY'));
    await pending;
  }
  assert.deepEqual(await ruler.findMatches(input), [
    { start: 0, end: 1, label: 'ORG', id: 'acme' }, { start: 0, end: 2, label: 'ORG', id: '' },
  ]);
  const token = input.token(0);
  const annotated = (await ruler.annotate(input)).toObject();
  assert.deepEqual(annotated.spanGroups, [{ key: 'ruler', name: 'ruler', attrs: {}, spans: [
    { start: 0, end: 1, label: 'ORG', id: 'acme' }, { start: 0, end: 2, label: 'ORG', id: '' },
  ] }]);
  assert.deepEqual(annotated.entities, [{ start: 0, end: 2, label: 'ORG', id: '' }]);
  // The input and its views keep their annotation.
  assert.deepEqual(input.toObject().spanGroups, []);
  assert.equal(input.toObject().entities, null);
  assert.deepEqual([token.annotations().entityType, token.annotations().entityIob], [null, null]);
  // Without a span group key, only entities are written.
  const entitiesOnly = new SpanRuler({ spansKey: null, annotateEntities: true });
  entitiesOnly.add([{ label: 'ORG', tokens: acme }]);
  const result = (await entitiesOnly.annotate(input)).toObject();
  assert.deepEqual([result.spanGroups, result.entities], [[], [{ start: 0, end: 1, label: 'ORG', id: '' }]]);
  ruler.clear();
  assert.deepEqual(state(ruler), { size: 0, labels: [], ids: [], patterns: [] });
  assert.equal(ruler.contains('ORG'), true);
  assert.equal(ruler.annotateEntities, true);
  assert.throws(() => Reflect.apply(ruler.annotate, ruler, [{}]), TypeError);
});

test('SpanRuler lexical flags use the model and survive clear', async () => {
  const model = await loadModel(modelPath);
  const input = await model.processDocument('Hi 42 there');
  const digits = await model.processDocument('1');
  const word: TokenPattern = { tokens: [{ constraints: [{ attribute: 'is_alpha', predicate: { kind: 'flag', value: true } }], repetition: { kind: 'once' } }] };
  const patterns: SpanPattern[] = [{ label: 'WORD', tokens: word }, { label: 'NUMBER', id: 'n', phrase: digits }];
  const options: SpanRulerOptions = { phraseAttribute: 'IS_ALPHA' };
  assert.throws(() => new SpanRuler(options).add(patterns), hasCode('SPARS_INVALID_PATTERN'));
  const ruler = new SpanRuler(options, model);
  const expected = [
    { start: 0, end: 1, label: 'WORD', id: '' }, { start: 1, end: 2, label: 'NUMBER', id: 'n' }, { start: 2, end: 3, label: 'WORD', id: '' },
  ];
  const spans = async (): Promise<unknown> => (await ruler.annotate(input)).toObject().spanGroups.find(group => group.key === 'ruler')?.spans;
  ruler.add(patterns);
  assert.deepEqual(await spans(), expected);
  ruler.clear();
  ruler.add(patterns);
  assert.deepEqual(await spans(), expected);
});

test('SpanRuler searches share admission, protect queued rules and validate their input', async () => {
  const model = await loadModel(modelPath);
  const input = await model.processDocument('Acme Corp', 'Tokenizer');
  for (const method of ['findMatches', 'annotate'] as const) {
    const ruler = new SpanRuler();
    const add = () => ruler.add([{ label: 'ORG', id: 'acme', tokens: acme }]);
    add();
    configureExecution({ maxActive: 1, maxQueued: 1 });
    const active = model.processDocument('Alice visits London.');
    // Queued in JavaScript, so only the wrapper, not the native handle, can reject changes.
    const queued = ruler[method](input);
    const settled = Promise.allSettled([active, queued]);
    try {
      assert.throws(add, hasCode('SPARS_BUSY'), method);
      assert.throws(() => ruler.remove('ORG'), hasCode('SPARS_BUSY'), method);
      assert.throws(() => ruler.removeById('acme'), hasCode('SPARS_BUSY'), method);
      assert.throws(() => ruler.clear(), hasCode('SPARS_BUSY'), method);
      // Reads still work, and other rulers can change.
      assert.deepEqual([ruler.size, ruler.contains('ORG'), ruler.patterns.length], [1, true, 1]);
      const other = new SpanRuler();
      other.add([{ label: 'X', tokens: acme }]);
      other.clear();
      await assert.rejects(ruler[method](input), hasCode('SPARS_BUSY'));
      await queued;
      ruler.removeById('acme');
      ruler.clear();
    } finally { await settled; configureExecution(null); }
    assert.throws(() => Reflect.apply(ruler[method], {}, [input]), TypeError);
    for (const value of [undefined, null, {}, input.toObject(), 'text']) {
      assert.throws(() => Reflect.apply(ruler[method], ruler, [value]), TypeError);
    }
    configureInputLimits({ maxTextLength: 1, maxBatchSize: 1, maxBatchTextLength: 1 });
    try {
      await assert.rejects(ruler[method](input), hasCode('SPARS_INPUT_LIMIT'));
    } finally { configureInputLimits(null); }
    // Rejected calls leave no pending search behind.
    add();
    ruler.clear();
    for (const mutate of [ruler.add, ruler.remove, ruler.removeById, ruler.clear]) {
      assert.throws(() => Reflect.apply(mutate, {}, [[]]), method);
    }
  }
});

test('SpanRuler search failures reject and release rule changes', async () => {
  // A lemma condition on a document without lemmas fails during matching.
  const lemma: TokenPattern = { tokens: [{ constraints: [{ attribute: 'lemma', predicate: { kind: 'equals', value: 'a' } }], repetition: { kind: 'once' } }] };
  for (const method of ['findMatches', 'annotate'] as const) {
    const ruler = new SpanRuler();
    ruler.add([{ label: 'X', id: 'x', tokens: lemma }]);
    await assert.rejects(ruler[method](doc(['a'])), hasCode('SPARS_INFERENCE'));
    ruler.removeById('x');
    assert.equal(ruler.size, 0);
  }
});
