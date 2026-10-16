import assert from 'node:assert/strict';
import { test } from 'node:test';
import { configureExecution, configureInputLimits, EntityRuler, loadModel, NativeDocument } from '../index.js';
import type { EntityPattern, EntityRulerOptions, RulerPattern, TokenPattern } from '../index.js';
import { array, modelPath, readJson, record, root } from './fixtures.mts';
import { member, PHRASE_ATTRIBUTES, recordedConstraint, repetition } from './vocabulary.mts';

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

/** Restore a recorded document. Snapshots before format 3 cannot carry entity IDs. */
function snapshot(value: unknown): NativeDocument {
  const document = record(value);
  const ids = array(document.tokens).some(t => record(t).entity_id != null)
    || (document.entities === null ? false : array(document.entities).some(e => record(e).id != null));
  return NativeDocument.fromSnapshot(JSON.stringify({ format_version: ids ? 3 : 1, document }));
}

/** The recorded ID, leaving the field out for none, as a caller would. */
function withId<T extends object>(base: T, id: string | null): T & { id?: string } {
  return id === null ? base : { ...base, id };
}

function pattern(value: unknown): EntityPattern {
  const p = record(value);
  const base = withId({ label: string(p.label) }, nullableString(p.id));
  if (p.tokens !== null) {
    assert.equal(p.phrase, null);
    return { ...base, tokens: tokenPattern(p.tokens) };
  }
  return { ...base, phrase: snapshot(p.phrase) };
}

function listed(value: unknown): RulerPattern {
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
  // Node reports a missing ID as '', as it does on entities.
  return { label: string(m.label), id: nullableString(m.id) ?? '', start: integer(m.start), end: integer(m.end) };
}

function options(value: unknown): EntityRulerOptions {
  const o = record(value);
  return { overwriteEntities: boolean(o.overwrite_entities), phraseAttribute: member(PHRASE_ATTRIBUTES, o.phrase_attribute),
    idSeparator: string(o.id_separator) };
}

function state(ruler: EntityRuler): unknown {
  return { size: ruler.size, labels: ruler.labels, ids: ruler.ids, patterns: ruler.patterns };
}

test('EntityRuler replays every frozen spaCy outcome', async () => {
  const fixture = record(readJson(`${root}fixtures/entity-ruler-v1.expected.json`));
  assert.equal(fixture.format_version, 1);
  assert.deepEqual(fixture.versions, { spacy: '3.8.14', en_core_web_md: '3.8.0' });
  let [cases, steps, applications, matches, errors] = [0, 0, 0, 0, 0];
  for (const value of array(fixture.cases)) {
    const c = record(value);
    cases += 1;
    const ruler = new EntityRuler(options(c.options));
    const initial = snapshot(c.document);
    let previous = initial;
    const outcomes = array(c.outcomes).map(record);
    for (const [index, stepValue] of array(c.steps).entries()) {
      const step = record(stepValue);
      const outcome = outcomes[index];
      assert.ok(outcome !== undefined);
      const at = `${string(c.id)} step ${index}`;
      steps += 1;
      let error: string | null = null;
      switch (step.action) {
        case 'add':
          ruler.add(array(step.patterns).map(pattern));
          break;
        case 'remove':
          try {
            ruler.remove(string(step.id));
          } catch (caught) {
            assert.ok(hasCode('SPARS_INVALID_PATTERN')(caught), at);
            error = 'ValueError';
          }
          break;
        case 'clear':
          ruler.clear();
          break;
        case 'apply': {
          applications += 1;
          const input = step.input === 'initial' ? initial : previous;
          assert.ok(step.input === 'initial' || step.input === 'previous', at);
          const before = input.toSnapshot();
          const expected = array(outcome.matches).map(matched);
          matches += expected.length;
          assert.deepEqual(await ruler.findMatches(input), expected, at);
          const annotated = await ruler.annotate(input);
          assert.deepEqual(annotated.toObject(), snapshot(outcome.result).toObject(), at);
          // Snapshots keep the ruler's entities and IDs, and the input is unchanged.
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
    }
  }
  assert.deepEqual([cases, steps, applications, matches, errors], [61, 161, 75, 176, 5]);
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

test('EntityRuler options default like spaCy and reject invalid settings', () => {
  const ruler = new EntityRuler();
  assert.deepEqual([ruler.overwriteEntities, ruler.phraseAttribute, ruler.idSeparator, ruler.size], [false, 'ORTH', '||', 0]);
  for (const options of [{}, null, undefined, { overwriteEntities: null, phraseAttribute: null, idSeparator: null }]) {
    const defaults = new EntityRuler(options);
    assert.deepEqual([defaults.overwriteEntities, defaults.phraseAttribute, defaults.idSeparator], [false, 'ORTH', '||']);
  }
  const custom = new EntityRuler({ overwriteEntities: true, phraseAttribute: 'LOWER', idSeparator: '#' });
  assert.deepEqual([custom.overwriteEntities, custom.phraseAttribute, custom.idSeparator], [true, 'LOWER', '#']);
  assert.throws(() => new EntityRuler({ idSeparator: '' }), hasCode('SPARS_INVALID_PATTERN'));
  assert.throws(() => Reflect.construct(EntityRuler, [{ phraseAttribute: 'SHAPE' }]), hasCode('SPARS_UNSUPPORTED'));
  assert.throws(() => Reflect.construct(EntityRuler, [{ phraseAttribute: 'lower' }]), hasCode('SPARS_UNSUPPORTED'));
  assert.throws(() => new EntityRuler({ idSeparator: '\uD800' }), hasCode('SPARS_INVALID_TEXT'));
  // Unknown options, such as spaCy's own names, and values of the wrong type are rejected.
  for (const options of [{ overwrite_ents: true }, { overwriteEnts: true }, { overwriteEntities: 'yes' }, { idSeparator: 5 }]) {
    assert.throws(() => Reflect.construct(EntityRuler, [options]), hasCode('InvalidArg'), JSON.stringify(options));
  }
  // Lexical flag phrases need a model; without one every phrase addition fails.
  const flags = new EntityRuler({ phraseAttribute: 'IS_ALPHA' });
  assert.throws(() => flags.add([{ label: 'X', phrase: doc(['a']) }]), hasCode('SPARS_INVALID_PATTERN'));
});

test('EntityRuler rejects malformed patterns without changing its rules', async () => {
  const ruler = new EntityRuler();
  ruler.add([{ label: 'ORG', id: 'acme', tokens: acme }]);
  const before = state(ruler);
  const valid: EntityPattern = { label: 'NEW', phrase: doc(['new']) };
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
  assert.throws(() => Reflect.apply(ruler.add, ruler, ['x']), hasCode('SPARS_INVALID_PATTERN'));
  // A phrase without the annotation the phrase attribute compares is an invalid pattern.
  const lemmas = new EntityRuler({ phraseAttribute: 'LEMMA' });
  assert.throws(() => lemmas.add([{ label: 'X', tokens: acme }, { label: 'X', phrase: doc(['a']) }]), hasCode('SPARS_INVALID_PATTERN'));
  assert.equal(lemmas.size, 0);
  assert.throws(() => ruler.add([{ label: '\uD800', tokens: acme }]), hasCode('SPARS_INVALID_TEXT'));
  assert.throws(() => ruler.remove('\uD800'), hasCode('SPARS_INVALID_TEXT'));
  assert.deepEqual(state(ruler), before);
  assert.throws(() => ruler.remove('missing'), hasCode('SPARS_INVALID_PATTERN'));
  ruler.remove('acme');
  assert.throws(() => ruler.remove('acme'), hasCode('SPARS_INVALID_PATTERN'));
  assert.deepEqual(await ruler.findMatches(doc(['Acme'])), []);
});

test('EntityRuler annotates copies and rejects rule changes while searches are pending', async () => {
  const ruler = new EntityRuler();
  ruler.add([{ label: 'ORG', id: 'acme', tokens: acme }, { label: 'ORG', phrase: doc(['Acme', 'Corp']) }]);
  assert.deepEqual(ruler.patterns, [{ label: 'ORG', id: 'acme', tokens: acme }, { label: 'ORG', phrase: 'Acme Corp' }]);
  const input = doc(['Acme', 'Corp', 'rocks']);
  // Start each search alone, so the checks do not depend on which one finishes first.
  for (const start of [() => ruler.findMatches(input), () => ruler.annotate(input)]) {
    const pending = start();
    assert.throws(() => ruler.add([{ label: 'X', tokens: acme }]), hasCode('SPARS_BUSY'));
    assert.throws(() => ruler.remove('acme'), hasCode('SPARS_BUSY'));
    assert.throws(() => ruler.clear(), hasCode('SPARS_BUSY'));
    await pending;
  }
  assert.deepEqual(await ruler.findMatches(input), [
    { label: 'ORG', id: '', start: 0, end: 2 }, { label: 'ORG', id: 'acme', start: 0, end: 1 },
  ]);
  const token = input.token(0);
  const span = input.span(0, 2);
  const annotated = await ruler.annotate(input);
  assert.deepEqual(annotated.toObject().entities, [{ start: 0, end: 2, label: 'ORG', id: '' }]);
  // The input and its views keep their annotation.
  assert.equal(input.toObject().entities, null);
  assert.equal(token.annotations().entityType, null);
  assert.equal(span.text, 'Acme Corp');
  ruler.clear();
  assert.deepEqual(state(ruler), { size: 0, labels: [], ids: [], patterns: [] });
  assert.equal(ruler.idSeparator, '||');
  assert.throws(() => Reflect.apply(ruler.annotate, ruler, [{}]), TypeError);
});

test('EntityRuler lexical flags use the model and survive clear', async () => {
  const model = await loadModel(modelPath);
  const input = await model.processDocument('Hi 42 there');
  const digits = await model.processDocument('1');
  const word: TokenPattern = { tokens: [{ constraints: [{ attribute: 'is_alpha', predicate: { kind: 'flag', value: true } }], repetition: { kind: 'once' } }] };
  const patterns: EntityPattern[] = [{ label: 'WORD', tokens: word }, { label: 'NUMBER', id: 'n', phrase: digits }];
  const settings: EntityRulerOptions = { overwriteEntities: true, phraseAttribute: 'IS_ALPHA' };
  assert.throws(() => new EntityRuler(settings).add(patterns), hasCode('SPARS_INVALID_PATTERN'));
  const ruler = new EntityRuler(settings, model);
  const expected = [
    { start: 0, end: 1, label: 'WORD', id: '' }, { start: 1, end: 2, label: 'NUMBER', id: 'n' }, { start: 2, end: 3, label: 'WORD', id: '' },
  ];
  ruler.add(patterns);
  assert.deepEqual((await ruler.annotate(input)).toObject().entities, expected);
  ruler.clear();
  ruler.add(patterns);
  assert.deepEqual((await ruler.annotate(input)).toObject().entities, expected);
});

test('EntityRuler searches share admission, protect queued rules and validate their input', async () => {
  const model = await loadModel(modelPath);
  const input = await model.processDocument('Acme Corp', 'Tokenizer');
  for (const method of ['findMatches', 'annotate'] as const) {
    const ruler = new EntityRuler();
    const add = () => ruler.add([{ label: 'ORG', id: 'acme', tokens: acme }]);
    add();
    configureExecution({ maxActive: 1, maxQueued: 1 });
    const active = model.processDocument('Alice visits London.');
    // Queued in JavaScript, so only the wrapper, not the native handle, can reject changes.
    const queued = ruler[method](input);
    const settled = Promise.allSettled([active, queued]);
    try {
      assert.throws(add, hasCode('SPARS_BUSY'), method);
      assert.throws(() => ruler.remove('acme'), hasCode('SPARS_BUSY'), method);
      assert.throws(() => ruler.clear(), hasCode('SPARS_BUSY'), method);
      await assert.rejects(ruler[method](input), hasCode('SPARS_BUSY'));
      await queued;
      ruler.remove('acme');
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
  }
});

test('EntityRuler search failures reject and release rule changes', async () => {
  // A lemma condition on a document without lemmas fails during matching.
  const lemma: TokenPattern = { tokens: [{ constraints: [{ attribute: 'lemma', predicate: { kind: 'equals', value: 'a' } }], repetition: { kind: 'once' } }] };
  for (const method of ['findMatches', 'annotate'] as const) {
    const ruler = new EntityRuler();
    ruler.add([{ label: 'X', id: 'x', tokens: lemma }]);
    await assert.rejects(ruler[method](doc(['a'])), hasCode('SPARS_INFERENCE'));
    ruler.remove('x');
    assert.equal(ruler.size, 0);
  }
});
