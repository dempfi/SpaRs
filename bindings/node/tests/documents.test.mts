import assert from 'node:assert/strict';
import { before, test } from 'node:test';
import { configureExecution, configureInputLimits, loadModel, Model, NativeDocument } from '../index.js';
import type { Stage } from '../index.js';
import { array, corpus, modelPath, record, root, snapshot } from './fixtures.mts';

let model: Model;
before(async () => { model = await loadModel(modelPath); });

test('retained documents share admission, settings protection and input limits', async () => {
  configureExecution({ maxActive: 1, maxQueued: 0 });
  try {
    const pending = model.processDocument('Alice visits London.');
    const done = Promise.allSettled([pending]);
    await assert.rejects(model.process('overflow'), { code: 'SPARS_BUSY' });
    assert.throws(() => { model.maxLength = 50; }, /idle/);
    await done;
    assert.equal((await pending).toObject().text, 'Alice visits London.');
    configureInputLimits({ maxTextLength: 1, maxBatchSize: 1, maxBatchTextLength: 1 });
    await assert.rejects(model.processDocument('😀'), { code: 'SPARS_INPUT_LIMIT' });
    assert.throws(() => Reflect.apply(model.processDocument, model, [42]), TypeError);
    assert.throws(() => Reflect.apply(model.processDocument, model, ['x', 'invalid']), TypeError);
  } finally { configureExecution(null); configureInputLimits(null); }
});

test('native document output matches all frozen official inference documents', async () => {
  let count = 0;
  for (const suite of ['development', 'holdout', 'regression', 'evaluation-v1', 'tokenizer-boundaries-v1', 'unseen-v1']) {
    for (const expected of corpus(suite)) {
      const doc = await model.processDocument(expected.text);
      assert.ok(doc instanceof NativeDocument);
      assert.deepEqual(snapshot(doc.toObject()), expected);
      assert.deepEqual(NativeDocument.fromSnapshot(doc.toSnapshot()).toObject(), doc.toObject());
      count++;
    }
  }
  assert.equal(count, 190);
});

test('native documents preserve stage availability, Unicode and empty inputs', async () => {
  const stages: Stage[] = ['Tokenizer', 'Tagger', 'Parser', 'AttributeRuler', 'Lemmatizer', 'Ner'];
  for (const stage of stages) {
    for (const text of ['', '😀 Café e\u0301.  ', '\r\n\t A\u00a0B 👩‍🔬!', 'a\0b']) {
      const doc = await model.processDocument(text, stage);
      assert.deepEqual(doc.toObject(), await model.process(text, stage));
      assert.deepEqual(NativeDocument.fromSnapshot(doc.toSnapshot()).toObject(), doc.toObject());
    }
  }
});

test('plain outputs cannot mutate retained documents and documents outlive models', async () => {
  let local: Model | undefined = await loadModel(modelPath);
  const pending = local.processDocument('Alice works in London.');
  local = undefined;
  global.gc?.();
  const doc = await pending;
  const expected = doc.toSnapshot();
  const output = doc.toObject();
  output.tokens.length = 0;
  output.text = 'changed';
  global.gc?.();
  assert.equal(doc.toSnapshot(), expected);
  assert.deepEqual(doc.toObject(), NativeDocument.fromSnapshot(expected).toObject());
});

test('small-model snapshots retain every contextual vector value', async () => {
  const small = await loadModel(`${root}assets/en_core_web_sm-3.8.0`);
  const doc = await small.processDocument('Alice works in London.');
  const serialized = doc.toSnapshot();
  const saved = record(JSON.parse(serialized));
  assert.equal(saved.format_version, 2);
  const tensor = array(record(saved.document).tensor);
  assert.equal(tensor.length, doc.toObject().tokens.length);
  assert.ok(array(tensor[0]).length > 0);
  const restored = NativeDocument.fromSnapshot(serialized);
  assert.equal(restored.toSnapshot(), serialized);
  assert.deepEqual(restored.toObject(), doc.toObject());
});

test('snapshots reject malformed JSON, unsupported versions, offsets, indices and tensors', async () => {
  assert.throws(() => NativeDocument.fromSnapshot('{'), { code: 'SPARS_INVALID_MODEL' });
  assert.throws(() => NativeDocument.fromSnapshot('\ud800'), { code: 'SPARS_INVALID_TEXT' });
  const doc = await model.processDocument('😀 A', 'Tokenizer');
  const encoded = doc.toSnapshot();
  const mutations: Array<(value: Record<string, unknown>) => void> = [
    value => { value.format_version = 99; },
    value => { record(array(record(value.document).tokens)[0]).end = 1; },
    value => { record(array(record(value.document).tokens)[0]).head = 99; },
    value => { record(value.document).sentences = [{ start: 0, end: 99, label: '' }]; },
    value => { value.format_version = 2; record(value.document).tensor = [[1], [1, 2]]; },
    value => { value.format_version = 2; record(value.document).tensor = [[1]]; },
    value => { value.format_version = 2; record(value.document).tensor = [[], []]; },
    value => { value.format_version = 2; record(value.document).tensor = [Array(4097).fill(0), Array(4097).fill(0)]; },
    value => { value.format_version = 2; record(value.document).tensor = [[null], [1]]; },
    value => { record(value.document).tensor = [[1], [2]]; },
  ];
  for (const mutate of mutations) {
    const malformed = record(JSON.parse(encoded));
    mutate(malformed);
    assert.throws(() => NativeDocument.fromSnapshot(JSON.stringify(malformed)));
  }
  const nonfinite = record(JSON.parse(encoded));
  nonfinite.format_version = 2;
  record(nonfinite.document).tensor = [[123456789], [1]];
  assert.throws(() => NativeDocument.fromSnapshot(JSON.stringify(nonfinite).replace('123456789', '1e400')), { code: 'SPARS_INVALID_MODEL' });
  assert.equal(doc.toSnapshot(), encoded);
});

test('snapshots keep and validate span groups written by Rust documents', async () => {
  const doc = await model.processDocument('New York City', 'Tokenizer');
  const saved = record(JSON.parse(doc.toSnapshot()));
  saved.format_version = 4;
  const groups = [
    { key: 'places', name: 'gazetteer', attrs: { source: { z: 1, a: [2] } }, spans: [{ start: 0, end: 3, label: 'GPE' }, { start: 1, end: 1, label: '' }] },
    { key: 'other', name: 'other', spans: [] },
  ];
  record(saved.document).span_groups = groups;
  const serialized = JSON.stringify(saved);
  const restored = NativeDocument.fromSnapshot(serialized);
  assert.equal(restored.toSnapshot(), serialized);
  assert.deepEqual(restored.toObject(), doc.toObject());
  for (const [version, invalid, code] of [
    [3, groups, 'SPARS_UNSUPPORTED'],
    [4, [{ key: 'a', name: 'a', spans: [{ start: 0, end: 4, label: '' }] }], 'SPARS_BOUNDS'],
    [4, [{ key: 'a', name: 'a', spans: [] }, { key: 'a', name: 'a', spans: [] }], 'SPARS_INVALID_MODEL'],
  ] as const) {
    const malformed = record(JSON.parse(serialized));
    malformed.format_version = version;
    record(malformed.document).span_groups = invalid;
    assert.throws(() => NativeDocument.fromSnapshot(JSON.stringify(malformed)), { code });
  }
});

test('native processing preserves text validation and code-point limits', async () => {
  await assert.rejects(model.processDocument('\ud800'), { code: 'SPARS_INVALID_TEXT' });
  const local = await loadModel(modelPath);
  local.maxLength = 1;
  assert.equal((await local.processDocument('😀', 'Tokenizer')).toObject().text, '😀');
  await assert.rejects(local.processDocument('😀A'), { code: 'SPARS_TEXT_TOO_LONG' });
});
