import assert from 'node:assert/strict';
import { before, test } from 'node:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { loadModel, Model, __napiBindingTarget } from '../index.js';
import type { Document, Stage } from '../index.js';
import { corpus, modelPath, root, snapshot, readJson, record, array } from './fixtures.mts';
import type { ReferenceDocument } from './fixtures.mts';

let model: Model;
before(async () => { assert.equal(__napiBindingTarget, 'native'); model = await loadModel(modelPath); });

test('every full-pipeline annotation matches the frozen official suites', async () => {
  const differences: { suite: string; expected: ReferenceDocument; actual: ReferenceDocument }[] = [];
  let documents = 0;
  let tokens = 0;
  for (const suite of ['development', 'holdout', 'regression', 'evaluation-v1', 'tokenizer-boundaries-v1', 'unseen-v1']) {
    for (const expected of corpus(suite)) {
      const result = await model.process(expected.text);
      checkOffsets(result);
      const actual = snapshot(result);
      documents++;
      tokens += expected.tokens.length;
      try { assert.deepEqual(actual, expected); }
      catch { differences.push({ suite, expected, actual }); }
      assert.equal(result.tokens.map(t => t.text + t.whitespace).join(''), expected.text);
    }
  }
  mkdirSync(`${root}target/reports`, { recursive: true });
  writeFileSync(`${root}target/reports/node-parity.json`, JSON.stringify({
    node: process.version, model: 'en_core_web_md 3.8.0', versions: { spacy: '3.8.14', thinc: '8.3.13' },
    command: 'npm --prefix bindings/node test', documents, tokens, differences,
  }, null, 2) + '\n');
  assert.equal(documents, 190);
  assert.equal(tokens, 7029);
  assert.equal(differences.length, 0, 'see target/reports/node-parity.json');
});

function checkOffsets(doc: Document): void {
  for (const [index, t] of doc.tokens.entries()) {
    assert.equal(t.index, index);
    assert.equal(doc.text.slice(t.utf16Start, t.utf16End), t.text);
    assert.equal(Buffer.from(doc.text).subarray(t.byteStart, t.byteEnd).toString('utf8'), t.text);
    assert.equal(Array.from(doc.text).slice(t.codePointStart, t.codePointEnd).join(''), t.text);
    assert.equal(Buffer.byteLength(doc.text.slice(0, t.utf16Start)), t.byteStart);
    assert.equal(Array.from(doc.text.slice(0, t.utf16Start)).length, t.codePointStart);
    assert.ok(t.whitespace === '' || t.whitespace === ' ');
  }
  assert.equal(doc.tokens.map(t => t.text + t.whitespace).join(''), doc.text);
}

test('Unicode and whitespace have exact byte, code-point, and UTF-16 offsets', async () => {
  for (const text of ['😀 Café e\u0301.  ', '\r\n\t A\u00a0B 👩‍🔬!', 'a\0b', '😀😀a', '', '  \t\n']) {
    const doc = await model.process(text);
    assert.equal(doc.text, text);
    checkOffsets(doc);
  }
});

test('ordered stages distinguish unavailable annotations from empty results', async () => {
  const stages: Stage[] = ['Tokenizer', 'Tagger', 'Parser', 'AttributeRuler', 'Lemmatizer', 'Ner'];
  for (const [index, stage] of stages.entries()) {
    const doc = await model.process('Dogs run.', stage);
    const first = doc.tokens[0];
    assert.ok(first);
    assert.equal(first.tag !== null, index >= 1);
    assert.equal(first.head !== null, index >= 2);
    assert.equal(first.sentenceStart !== null, index >= 2);
    assert.equal(doc.sentences !== null, index >= 2);
    assert.equal(first.pos !== null, index >= 3);
    assert.equal(first.morphology !== null, index >= 3);
    assert.equal(doc.nounChunks !== null, index >= 3);
    assert.equal(first.lemma !== null, index >= 4);
    assert.equal(first.entityIob !== null, index >= 5);
    assert.equal(doc.entities !== null, index >= 5);
  }
  assert.deepEqual(await model.process(''), { text: '', tokens: [], entities: [], sentences: [], nounChunks: [], spanGroups: [] });
  assert.deepEqual(await model.process('', 'Tokenizer'), { text: '', tokens: [], entities: null, sentences: null, nounChunks: null, spanGroups: [] });
});

test('repeated, batched and concurrent calls remain independent', async () => {
  const texts = ['Alice works in London.', '', '😀 Café e\u0301.', 'They did not leave.'];
  const expected = await model.processBatch(texts);
  assert.deepEqual(await model.processBatch([]), []);
  assert.deepEqual(await model.processBatch(texts), expected);
  assert.deepEqual(await Promise.all(texts.map(text => model.process(text))), expected);
  assert.deepEqual(await model.processBatch(texts, 'Tokenizer'), await Promise.all(texts.map(text => model.process(text, 'Tokenizer'))));
  const changed = await model.process('Alice works in London.');
  const first = changed.tokens[0];
  assert.ok(first);
  first.lemma = 'mutated';
  changed.tokens.length = 0;
  assert.deepEqual(await model.process('Alice works in London.'), expected[0]);
});

test('workers retain the model through garbage collection and leave the event loop responsive', async () => {
  let local: Model | undefined = await loadModel(modelPath);
  const text = 'Alice works in London. '.repeat(100);
  const pending = local.processBatch(Array.from({ length: 12 }, () => text));
  local = undefined;
  assert.ok(global.gc, 'test runner must enable --expose-gc');
  global.gc();
  let ticks = 0;
  const timer = setInterval(() => { ticks++; }, 1);
  try {
    const docs = await pending;
    assert.equal(docs.length, 12);
    assert.equal(docs[0]?.text, text);
    assert.ok(ticks > 0, 'inference must not block the JavaScript event loop');
  } finally { clearInterval(timer); }
});

test('static vectors are independent Float32Array values with explicit absence', () => {
  const fixture = record(readJson(`${root}fixtures/vectors.expected.json`));
  const words = array(fixture.words);
  assert.equal(words.length, 8);
  for (const value of words) {
    const row = record(value);
    assert.ok(typeof row.text === 'string');
    assert.ok(typeof row.has_vector === 'boolean');
    const actual = model.vector(row.text);
    if (!row.has_vector) assert.equal(actual, null);
    else {
      assert.ok(actual instanceof Float32Array);
      const expected = array(row.vector).map(value => { assert.ok(typeof value === 'number'); return Math.fround(value); });
      assert.deepEqual(Array.from(actual), expected);
    }
  }
  const vector = model.vector('dog');
  assert.ok(vector instanceof Float32Array);
  assert.equal(vector.length, 300);
  const first = vector[0];
  vector[0] = 999;
  assert.equal(model.vector('dog')?.[0], first);
  assert.equal(model.vector('spars_unknown_🙂_lexeme'), null);
  assert.equal(model.vector(''), null);
});

test('a separate Node consumer works with an empty PATH', () => {
  const result = spawnSync(process.execPath, ['bindings/node/scripts/smoke.mts', modelPath], {
    cwd: root, env: { PATH: '' }, encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /PASS native Node inference/);
});

test('invalid UTF-16 is rejected without replacing original text', async () => {
  for (const text of ['\ud800', '\udc00', 'a\ud800b']) {
    await assert.rejects(model.process(text), { code: 'SPARS_INVALID_TEXT' });
    await assert.rejects(model.processBatch(['ok', text]), { code: 'SPARS_INVALID_TEXT' });
    await assert.rejects(loadModel(text), { code: 'SPARS_INVALID_TEXT' });
    assert.throws(() => model.vector(text), { code: 'SPARS_INVALID_TEXT' });
  }
  assert.equal((await model.process('still usable')).text, 'still usable');
});

test('JavaScript argument types and stages are validated at the native boundary', () => {
  assert.throws(() => Reflect.construct(Model, []), /constructor/);
  // Deliberately bypass TypeScript only for malformed external input tests.
  const invoke = (name: 'process' | 'processBatch' | 'vector', args: unknown[]): unknown => Reflect.apply(model[name], model, args);
  for (const bad of [null, undefined, 42, {}, ['text']]) {
    assert.throws(() => invoke('process', [bad]));
    assert.throws(() => invoke('vector', [bad]));
  }
  for (const bad of [42, {}, 'text', [null], [42], ['ok', {}]]) assert.throws(() => invoke('processBatch', [bad]));
  for (const bad of ['Unknown', 'ner', 5, {}]) assert.throws(() => invoke('process', ['text', bad]));
  assert.throws(() => Reflect.apply(loadModel, undefined, [42]));
});

test('missing, malformed and unsupported assets return useful error codes', async () => {
  await assert.rejects(loadModel(`${root}target/absent-node-model`), { code: 'SPARS_IO' });
  mkdirSync(`${root}target`, { recursive: true });
  const folder = mkdtempSync(`${root}target/node-malformed-`);
  try {
    writeFileSync(`${folder}/manifest.json`, '{');
    await assert.rejects(loadModel(folder), { code: 'SPARS_INVALID_MODEL' });
    const manifest = record(readJson(`${modelPath}/manifest.json`));
    writeFileSync(`${folder}/manifest.json`, JSON.stringify(manifest));
    writeFileSync(`${folder}/weights.safetensors`, 'corrupt');
    await assert.rejects(loadModel(folder), { code: 'SPARS_INVALID_MODEL', message: /checksum/ });
    writeFileSync(`${folder}/manifest.json`, JSON.stringify({ ...manifest, config: '[nlp]\nbatch_size = 0' }));
    await assert.rejects(loadModel(folder), { code: 'SPARS_INVALID_MODEL', message: /batch_size/ });
    writeFileSync(`${folder}/manifest.json`, JSON.stringify({ ...manifest, format_version: 999 }));
    await assert.rejects(loadModel(folder), { code: 'SPARS_UNSUPPORTED' });
  } finally { rmSync(folder, { recursive: true }); }
});
