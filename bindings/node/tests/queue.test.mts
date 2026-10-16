import assert from 'node:assert/strict';
import { test } from 'node:test';
import { configureExecution, loadModel } from '../index.js';
import { Scheduler, install } from '../execution.cjs';
import type { Document } from '../index.js';
import { Model } from '../index.js';
import { modelPath } from './fixtures.mts';

test('admission is shared across models, rejects overflow, and recovers after errors', async () => {
  const first = await loadModel(modelPath);
  const second = await loadModel(modelPath);
  configureExecution({ maxActive: 1, maxQueued: 1 });
  try {
    const active = first.process('Alice visits London.');
    const textsWaiting = ['Dogs run.'];
    const queued = second.processBatch(textsWaiting);
    textsWaiting[0] = 'changed';
    await assert.rejects(first.process('overflow'), { code: 'SPARS_BUSY' });
    assert.throws(() => configureExecution({ maxActive: 2, maxQueued: 2 }), /idle/);
    assert.equal((await active).text, 'Alice visits London.');
    assert.equal((await queued)[0]?.text, 'Dogs run.');
    const invalid = first.process('\ud800');
    const next = second.process('Still works.');
    await assert.rejects(invalid, { code: 'SPARS_INVALID_TEXT' });
    assert.equal((await next).text, 'Still works.');
    install({ Model }); // Reinstalling must not wrap an already verified loader twice.
    configureExecution({ maxActive: 1, maxQueued: 0 });
    const texts = ['original'];
    const batch = first.processBatch(texts);
    texts[0] = 'changed';
    await assert.rejects(second.processBatch(['overflow']), { code: 'SPARS_BUSY' });
    assert.equal((await batch)[0]?.text, 'original');
  } finally { configureExecution(null); }
});

function deferred(): { promise: Promise<number>; resolve: (value: number) => void; reject: (error: Error) => void } {
  let resolve: (value: number) => void = () => { throw new Error('not initialized'); };
  let reject: (error: Error) => void = () => { throw new Error('not initialized'); };
  const promise = new Promise<number>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

test('scheduler caps native submissions, preserves FIFO, and releases every slot', async () => {
  const scheduler = new Scheduler();
  scheduler.configure({ maxActive: 2, maxQueued: 2 });
  const work = [deferred(), deferred(), deferred(), deferred()];
  const started: number[] = [];
  const jobs = work.map((item, index) => scheduler.submit(() => { started.push(index); return item.promise; }));
  const settled = Promise.allSettled(jobs);
  assert.deepEqual(started, [0, 1]);
  await assert.rejects(scheduler.submit(() => { throw new Error('must not start'); }), { code: 'SPARS_BUSY' });
  work[1]?.resolve(1);
  assert.equal(await jobs[1], 1);
  assert.deepEqual(started, [0, 1, 2]);
  work[0]?.reject(new Error('native failure'));
  await assert.rejects(jobs[0]!, /native failure/);
  assert.deepEqual(started, [0, 1, 2, 3]);
  work[2]?.resolve(2); work[3]?.resolve(3);
  await settled;
  assert.equal(scheduler.active, 0);
  assert.equal(scheduler.queued, 0);
  await assert.rejects(scheduler.submit(() => { throw new Error('sync failure'); }), /sync failure/);
  assert.equal(await scheduler.submit(async () => 42), 42);
  scheduler.configure({ maxActive: 1, maxQueued: 0 });
});

test('invalid configuration leaves defaults unchanged', () => {
  for (const options of [[], {}, {maxActive: 0,maxQueued: 1}, {maxActive: 1.5,maxQueued: 1},
    {maxActive: 1,maxQueued: -1}, {maxActive: 1,maxQueued: Infinity}, {maxActive: '1',maxQueued: 1},
    {maxActive: Number.MAX_SAFE_INTEGER + 1,maxQueued: 0}]) {
    const scheduler = new Scheduler();
    assert.throws(() => Reflect.apply(scheduler.configure, scheduler, [options]));
    assert.equal(scheduler.maxActive, 2);
    assert.equal(scheduler.maxQueued, 32);
  }
});

test('wrappers defer native invocation and reject saturation without reading batch elements', async () => {
  const started: string[] = [];
  const gates = new Map<string, { promise: Promise<Document>; resolve: (value: Document) => void }>();
  for (const key of ['first', 'second', 'third']) {
    let resolve: (value: Document) => void = () => { throw new Error('not initialized'); };
    const promise = new Promise<Document>(accept => { resolve = accept; });
    gates.set(key, { promise, resolve });
  }
  class FakeModel {
    private maximum = 1_000_000;
    private size = 256;
    get maxLength(): number { return this.maximum; }
    set maxLength(value: number) { this.maximum = value; }
    get batchSize(): number { return this.size; }
    set batchSize(value: number) { this.size = value; }
    async *pipe(): AsyncGenerator<Document, void, unknown> { yield await this.process('first'); }
    process(text: string): Promise<Document> {
      started.push(text);
      const gate = gates.get(text);
      assert.ok(gate);
      return gate.promise;
    }
    async processBatch(texts: string[]): Promise<Document[]> {
      const text = texts[0];
      assert.ok(text);
      return [await FakeModelNative.call(this, text)];
    }
    vector(): null { return null; }
  }
  const FakeModelNative = FakeModel.prototype.process;
  install({ Model: FakeModel });
  configureExecution({ maxActive: 1, maxQueued: 2 });
  const model = new FakeModel();
  const first = model.process('first');
  const second = model.processBatch(['second']);
  const third = model.process('third');
  assert.deepEqual(started, ['first']);
  const inaccessible = ['never'];
  Object.defineProperty(inaccessible, 0, { get() { throw new Error('must not copy a rejected batch'); } });
  await assert.rejects(model.processBatch(inaccessible), { code: 'SPARS_BUSY' });
  assert.deepEqual(started, ['first']);
  const doc = (text: string): Document => ({ text, tokens: [], entities: null, sentences: null, nounChunks: null, spanGroups: [] });
  gates.get('first')?.resolve(doc('first'));
  await first;
  assert.deepEqual(started, ['first', 'second']);
  gates.get('second')?.resolve(doc('second'));
  await second;
  assert.deepEqual(started, ['first', 'second', 'third']);
  gates.get('third')?.resolve(doc('third'));
  await third;
  configureExecution(null);
});


test('reset restores default admission caps for subsequent submissions', async () => {
  const model = await loadModel(modelPath);
  configureExecution({ maxActive: 1, maxQueued: 0 });
  const first = model.process('a', 'Tokenizer');
  try {
    await assert.rejects(model.process('b', 'Tokenizer'), { code: 'SPARS_BUSY' });
    await first;
    configureExecution(null);
    const accepted = Array.from({ length: 34 }, () => model.process('a', 'Tokenizer'));
    const settled = Promise.allSettled(accepted);
    await assert.rejects(model.process('overflow'), { code: 'SPARS_BUSY' });
    assert.ok((await settled).every(result => result.status === 'fulfilled'));
    assert.equal((await model.process('recovered', 'Tokenizer')).text, 'recovered');
  } finally { await first; configureExecution(null); }
});

test('native chunks run sequentially under one admission slot and stop on failure', async () => {
  const chunks: string[][] = [];
  let gates = [deferred(), deferred(), deferred()];
  let active = 0;
  const doc = (text: string): Document => ({ text, tokens: [], entities: null, sentences: null, nounChunks: null, spanGroups: [] });
  class FakeModel {
    private maximum = 1_000_000;
    private size = 2;
    get maxLength(): number { return this.maximum; }
    set maxLength(value: number) { this.maximum = value; }
    get batchSize(): number { return this.size; }
    set batchSize(value: number) { this.size = value; }
    async *pipe(): AsyncGenerator<Document, void, unknown> { yield doc('unused'); }
    async process(text: string): Promise<Document> { return doc(text); }
    async processBatch(texts: string[]): Promise<Document[]> {
      assert.equal(active, 0, 'native chunks must not overlap');
      const gate = gates[chunks.length];
      assert.ok(gate);
      chunks.push(texts);
      active++;
      try { await gate.promise; return texts.map(doc); }
      finally { active--; }
    }
    vector(): null { return null; }
  }
  install({ Model: FakeModel });
  const model = new FakeModel();
  configureExecution({ maxActive: 1, maxQueued: 0 });
  try {
    const texts = ['a', 'b', 'c', 'd', 'e'];
    const result = model.processBatch(texts);
    texts[2] = 'changed';
    assert.deepEqual(chunks, [['a', 'b']]);
    gates[0]?.resolve(0);
    await new Promise<void>(resolve => setImmediate(resolve));
    assert.deepEqual(chunks, [['a', 'b'], ['c', 'd']]);
    await assert.rejects(model.process('overflow'), { code: 'SPARS_BUSY' });
    gates[1]?.resolve(1);
    await new Promise<void>(resolve => setImmediate(resolve));
    assert.deepEqual(chunks, [['a', 'b'], ['c', 'd'], ['e']]);
    await assert.rejects(model.process('overflow'), { code: 'SPARS_BUSY' });
    gates[2]?.resolve(2);
    assert.deepEqual((await result).map(item => item.text), ['a', 'b', 'c', 'd', 'e']);
    chunks.length = 0;
    gates = [deferred(), deferred(), deferred()];
    const failure = model.processBatch(['a', 'b', 'c', 'd', 'e']);
    const rejected = assert.rejects(failure, /middle chunk failed/);
    gates[0]?.resolve(0);
    await new Promise<void>(resolve => setImmediate(resolve));
    gates[1]?.reject(new Error('middle chunk failed'));
    await rejected;
    assert.deepEqual(chunks, [['a', 'b'], ['c', 'd']]);
    assert.equal((await model.process('recovered')).text, 'recovered');
  } finally { configureExecution(null); }
});


test('fresh scheduler admits two jobs and queues exactly 32 without configuration', async () => {
  const scheduler = new Scheduler();
  const gate = deferred();
  const started: number[] = [];
  const accepted = Array.from({ length: 34 }, (_, index) => scheduler.submit(() => {
    started.push(index);
    return gate.promise;
  }));
  const settled = Promise.allSettled(accepted);
  try {
    assert.deepEqual(started, [0, 1]);
    assert.equal(scheduler.active, 2);
    assert.equal(scheduler.queued, 32);
    await assert.rejects(scheduler.submit(async () => 35), { code: 'SPARS_BUSY' });
  } finally { gate.resolve(0); await settled; }
  assert.deepEqual(started, Array.from({ length: 34 }, (_, index) => index));
  scheduler.configure({ maxActive: 4, maxQueued: 0 });
  scheduler.configure(null);
  assert.equal(scheduler.maxActive, 2);
  assert.equal(scheduler.maxQueued, 32);
});


test('an incompatible native addon fails before any inference method is wrapped', () => {
  for (const variant of ['missingBoth', 'missingBatch', 'readOnlyBatch']) {
    class LegacyModel {
      process(): void { throw new Error('must not execute'); }
      processBatch(): void { throw new Error('must not execute'); }
    }
    if (variant !== 'missingBoth') Object.defineProperty(LegacyModel.prototype, 'maxLength', {
      configurable: true, get() { return 1_000_000; }, set(_value: number) {},
    });
    if (variant === 'readOnlyBatch') Object.defineProperty(LegacyModel.prototype, 'batchSize', {
      configurable: true, get() { return 256; },
    });
    const descriptors = Object.getOwnPropertyDescriptors(LegacyModel.prototype);
    for (let attempt = 0; attempt < 2; attempt++) {
      assert.throws(() => Reflect.apply(install, undefined, [{ Model: LegacyModel }]), {
        code: 'SPARS_NATIVE_INCOMPATIBLE', message: /NAPI_RS_NATIVE_LIBRARY_PATH/,
      });
      assert.deepEqual(Object.getOwnPropertyDescriptors(LegacyModel.prototype), descriptors);
    }
  }
});
