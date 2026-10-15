// @ts-check
'use strict';

/** @typedef {import('./index.js').ExecutionOptions} ExecutionOptions */
/** @typedef {import('./index.js').InputLimits} InputLimits */
/** @typedef {{ start: () => void, next: Job | null }} Job */

const DEFAULT_MAX_ACTIVE = 2;
const DEFAULT_MAX_QUEUED = 32;

/** A bounded FIFO that submits only active jobs to the native worker pool. */
class Scheduler {
  /** @type {number} */
  active = 0;
  /** @type {number} */
  queued = 0;
  /** @type {Job | null} */
  head = null;
  /** @type {Job | null} */
  tail = null;
  maxActive = DEFAULT_MAX_ACTIVE;
  maxQueued = DEFAULT_MAX_QUEUED;

  assertIdle() {
    if (this.active || this.queued) throw new Error('Configure processing only while inference is idle');
  }

  /** @param {ExecutionOptions | null} options */
  configure(options) {
    if (options === null) {
      this.assertIdle();
      this.maxActive = DEFAULT_MAX_ACTIVE;
      this.maxQueued = DEFAULT_MAX_QUEUED;
      return;
    }
    if (!options || typeof options !== 'object' || Array.isArray(options)) throw new TypeError('Expected execution options');
    const { maxActive, maxQueued } = options;
    if (!Number.isSafeInteger(maxActive) || maxActive < 1 || !Number.isSafeInteger(maxQueued) || maxQueued < 0) {
      throw new RangeError('maxActive must be a positive safe integer; maxQueued must be a nonnegative safe integer');
    }
    this.assertIdle();
    this.maxActive = maxActive;
    this.maxQueued = maxQueued;
  }

  full() { return this.active >= this.maxActive && this.queued >= this.maxQueued; }

  /** @template T @param {() => Promise<T>} run @returns {Promise<T>} */
  submit(run) {
    if (this.full()) {
      return busy();
    }
    return new Promise((resolve, reject) => {
      const start = () => {
        this.active++;
        // A synchronous native failure must also return its admission slot.
        try {
          run().then(value => { this.finish(); resolve(value); }, error => { this.finish(); reject(error); });
        } catch (error) { this.finish(); reject(error); }
      };
      if (this.active < this.maxActive) start();
      else {
        const job = { start, next: null };
        if (this.tail) this.tail.next = job;
        else this.head = job;
        this.tail = job;
        this.queued++;
      }
    });
  }

  finish() {
    this.active--;
    const job = this.head;
    if (job) {
      this.head = job.next;
      if (!this.head) this.tail = null;
      this.queued--;
      job.start();
    }
  }
}

const ERROR_CODES = new Set(['SPARS_IO', 'SPARS_INVALID_MODEL', 'SPARS_UNSUPPORTED', 'SPARS_INVALID_TEXT', 'SPARS_TEXT_TOO_LONG',
  'SPARS_BOUNDS', 'SPARS_INFERENCE', 'SPARS_INVALID_PATTERN', 'SPARS_BUSY', 'SPARS_INPUT_LIMIT', 'SPARS_NATIVE_INCOMPATIBLE']);

const OFFICIAL_MODEL_NAMES = new Set(['en_core_web_sm', 'en_core_web_md', 'en_core_web_lg']);

/**
 * @param {string} name
 * @returns {boolean}
 */
function isOfficialModelName(name) { return OFFICIAL_MODEL_NAMES.has(name); }

/**
 * @param {unknown} error
 * @returns {boolean}
 */
function isSparsError(error) {
  return error instanceof Error && 'code' in error && typeof error.code === 'string' && ERROR_CODES.has(error.code);
}

/** @returns {Promise<never>} */
function busy() { return Promise.reject(Object.assign(new Error('Inference queue is full'), { code: 'SPARS_BUSY' })); }

const scheduler = new Scheduler();
/** @type {InputLimits | null} */
let inputLimits = null;

/** @param {InputLimits | null} options */
function configureInputLimits(options) {
  if (options === null) { scheduler.assertIdle(); inputLimits = null; return; }
  if (!options || typeof options !== 'object' || Array.isArray(options)) throw new TypeError('Expected input limits');
  const { maxTextLength, maxBatchSize, maxBatchTextLength } = options;
  if (![maxTextLength, maxBatchSize, maxBatchTextLength].every(value => Number.isSafeInteger(value) && value > 0)) {
    throw new RangeError('Input limits must be positive safe integers');
  }
  scheduler.assertIdle();
  inputLimits = { maxTextLength, maxBatchSize, maxBatchTextLength };
}

/** @param {string} message @returns {Promise<never>} */
function tooLarge(message) {
  return Promise.reject(Object.assign(new Error(message), { code: 'SPARS_INPUT_LIMIT' }));
}

/** @param {ExecutionOptions | null} options */
function configureExecution(options) { scheduler.configure(options); }

/** @type {WeakSet<object>} */
const installed = new WeakSet();

/** @param {unknown} stage */
function validateStage(stage) {
  if (stage != null && (typeof stage !== 'string' || !['Tokenizer', 'Tagger', 'Parser', 'AttributeRuler', 'Lemmatizer', 'Ner'].includes(stage))) {
    throw new TypeError('Unknown pipeline stage');
  }
}

/** @typedef {Pick<import('./index.js').Model, 'process' | 'processBatch' | 'pipe' | 'maxLength' | 'batchSize'> & Partial<Pick<import('./index.js').Model, 'processDocument'>>} ProcessingModel */
/** @param {{ Model: { prototype: ProcessingModel; [Symbol.hasInstance](value: unknown): boolean } }} binding */
function install(binding) {
  const prototype = binding.Model.prototype;
  if (installed.has(prototype)) return;
  // An older override can load successfully but lacks settings needed for chunking.
  // Validate every required accessor before changing any native method or descriptor.
  for (const name of ['maxLength', 'batchSize']) {
    const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
    if (typeof descriptor?.get !== 'function' || typeof descriptor?.set !== 'function') {
      throw Object.assign(new Error('Native addon is incompatible with this @spars/node wrapper. Reinstall matching package versions and remove or update NAPI_RS_NATIVE_LIBRARY_PATH.'), {
        code: 'SPARS_NATIVE_INCOMPATIBLE',
      });
    }
  }
  installed.add(prototype);
  // Freeze pipeline settings during submitted work, including JS-queued jobs.
  for (const name of ['maxLength', 'batchSize']) {
    const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
    const set = descriptor?.set;
    if (descriptor && set) Object.defineProperty(prototype, name, {
      ...descriptor,
      set(value) { scheduler.assertIdle(); set.call(this, value); },
    });
  }
  const process = prototype.process;
  const batch = prototype.processBatch;
  prototype.process = function(text, stage) {
    if (!(this instanceof binding.Model)) throw new TypeError('Expected a Model receiver');
    if (typeof text !== 'string') throw new TypeError('Expected a text string');
    validateStage(stage);
    if (inputLimits && text.length > inputLimits.maxTextLength) return tooLarge('Text exceeds maxTextLength (UTF-16 units)');
    return scheduler.submit(() => process.call(this, text, stage));
  };
  const processDocument = prototype.processDocument;
  if (processDocument) prototype.processDocument = function(text, stage) {
    if (!(this instanceof binding.Model)) throw new TypeError('Expected a Model receiver');
    if (typeof text !== 'string') throw new TypeError('Expected a text string');
    validateStage(stage);
    if (inputLimits && text.length > inputLimits.maxTextLength) return tooLarge('Text exceeds maxTextLength (UTF-16 units)');
    return scheduler.submit(() => processDocument.call(this, text, stage));
  };
  prototype.processBatch = function(texts, stage) {
    if (!(this instanceof binding.Model)) throw new TypeError('Expected a Model receiver');
    if (!Array.isArray(texts)) throw new TypeError('Expected an array of text strings');
    validateStage(stage);
    const limits = inputLimits;
    const size = this.batchSize;
    const count = texts.length;
    if (limits && count > limits.maxBatchSize) return tooLarge('Batch exceeds maxBatchSize');
    if (scheduler.full()) return busy();
    // Capture strings at submission, including for jobs that wait in JavaScript.
    let remaining = limits?.maxBatchTextLength ?? Infinity;
    /** @type {string[]} */
    const snapshot = [];
    for (let i = 0; i < count; i++) {
      const text = texts[i];
      if (typeof text !== 'string') throw new TypeError('Expected a text string');
      if (limits && text.length > limits.maxTextLength) return tooLarge('Batch text exceeds maxTextLength (UTF-16 units)');
      if (text.length > remaining) return tooLarge('Batch exceeds maxBatchTextLength (UTF-16 units)');
      remaining -= text.length;
      snapshot.push(text);
    }
    return scheduler.submit(async () => {
      /** @type {import('./index.js').Document[]} */
      const documents = [];
      for (let offset = 0; offset < snapshot.length; offset += size) {
        const result = await batch.call(this, snapshot.slice(offset, offset + size), stage);
        for (const doc of result) documents.push(doc);
      }
      return documents;
    });
  };
  prototype.pipe = async function*(texts, options = {}) {
    if (!(this instanceof binding.Model)) throw new TypeError('Expected a Model receiver');
    if (!options || typeof options !== 'object' || Array.isArray(options)) throw new TypeError('Expected pipe options');
    for (const key of Object.keys(options)) {
      if (key !== 'batchSize' && key !== 'stage') throw new TypeError(`Unsupported pipe option: ${key}`);
    }
    const size = options.batchSize ?? this.batchSize;
    const stage = options.stage;
    if (!Number.isInteger(size) || size < 1 || size > 0xffffffff) throw new RangeError('batchSize must be a positive u32 integer');
    validateStage(stage);
    /** @type {string[]} */
    let buffer = [];
    for await (const text of texts) {
      if (typeof text !== 'string') throw new TypeError('Expected a text string');
      buffer.push(text);
      if (buffer.length === size) {
        const docs = await this.processBatch(buffer, stage);
        buffer = [];
        yield* docs;
      }
    }
    if (buffer.length) yield* await this.processBatch(buffer, stage);
  };
}

/**
 * Run document searches through the scheduler and count them per receiver, so rule changes can
 * be rejected while a search is queued or running.
 * @template {string} S
 * @param {Record<S, (doc: import('./index.js').NativeDocument) => Promise<unknown>>} prototype
 * @param {{ [Symbol.hasInstance](value: unknown): boolean }} Receiver
 * @param {typeof import('./index.js').NativeDocument} NativeDocument
 * @param {S[]} searches
 * @returns {(receiver: object) => void} a check that throws SPARS_BUSY while searches are pending
 */
function guardSearches(prototype, Receiver, NativeDocument, searches) {
  /** @type {WeakMap<object, number>} */
  const pending = new WeakMap();
  for (const name of searches) {
    const search = prototype[name];
    if (typeof search !== 'function') throw new TypeError(`Expected a native ${name} method`);
    /** @this {object} @param {import('./index.js').NativeDocument} doc */
    prototype[name] = function(doc) {
      if (!(this instanceof Receiver)) throw new TypeError('Expected a matcher receiver');
      if (!(doc instanceof NativeDocument)) throw new TypeError('Expected a NativeDocument');
      if (inputLimits && doc.utf16Length > inputLimits.maxTextLength) return tooLarge('Document exceeds maxTextLength (UTF-16 units)');
      if (scheduler.full()) return busy();
      pending.set(this, (pending.get(this) ?? 0) + 1);
      return scheduler.submit(() => search.call(this, doc)).finally(() => {
        const count = (pending.get(this) ?? 1) - 1;
        if (count) pending.set(this, count);
        else pending.delete(this);
      });
    };
  }
  return receiver => {
    if (pending.has(receiver)) throw Object.assign(new Error('Matcher rules cannot change while matching is pending'), { code: 'SPARS_BUSY' });
  };
}

/**
 * Keep rule mutation excluded from queued as well as actively executing searches.
 * @template T
 * @template {unknown[]} A
 * @param {{prototype: {findMatches(doc: import('./index.js').NativeDocument): Promise<T[]>; add(...args: A): void; remove(rule: string): void}; [Symbol.hasInstance](value: unknown): boolean}} Matcher
 * @param {typeof import('./index.js').NativeDocument} NativeDocument
 * @param {'phrase' | 'token' | 'dependency'} kind
 */
function installMatcher(Matcher, NativeDocument, kind) {
  const prototype = Matcher.prototype;
  if (installed.has(prototype)) return;
  installed.add(prototype);
  const add = prototype.add;
  const remove = prototype.remove;
  const assertMutable = guardSearches(prototype, Matcher, NativeDocument, ['findMatches']);
  prototype.add = function(...args) {
    assertMutable(this);
    if (kind !== 'phrase') validatePatterns(args[1], kind);
    add.apply(this, args);
  };
  prototype.remove = function(rule) { assertMutable(this); remove.call(this, rule); };
}

/**
 * Queue ruler searches like matcher searches, and reject pattern changes while one is pending.
 * @param {typeof import('./index.js').EntityRuler} EntityRuler
 * @param {typeof import('./index.js').NativeDocument} NativeDocument
 */
function installEntityRuler(EntityRuler, NativeDocument) {
  const prototype = EntityRuler.prototype;
  if (installed.has(prototype)) return;
  installed.add(prototype);
  const add = prototype.add;
  const remove = prototype.remove;
  const clear = prototype.clear;
  const assertMutable = guardSearches(prototype, EntityRuler, NativeDocument, ['findMatches', 'annotate']);
  prototype.add = function(patterns) {
    assertMutable(this);
    for (const value of patternArray(patterns)) {
      const pattern = patternRecord(value, ['label', 'id', 'tokens', 'phrase']);
      if (typeof pattern.label !== 'string' || (pattern.id != null && typeof pattern.id !== 'string')) {
        throw Object.assign(new Error('An entity pattern label and id must be strings'), { code: 'SPARS_INVALID_PATTERN' });
      }
      const tokens = pattern.tokens != null;
      const phrase = pattern.phrase != null;
      if (tokens === phrase) throw Object.assign(new Error('An entity pattern needs exactly one of tokens and phrase'), { code: 'SPARS_INVALID_PATTERN' });
      if (tokens) validatePatterns([pattern.tokens], 'token');
      else if (!(pattern.phrase instanceof NativeDocument)) throw Object.assign(new Error('An entity phrase must be a NativeDocument'), { code: 'SPARS_INVALID_PATTERN' });
    }
    add.call(this, patterns);
  };
  prototype.remove = function(id) { assertMutable(this); remove.call(this, id); };
  prototype.clear = function() { assertMutable(this); clear.call(this); };
}

/** @param {unknown} value @param {string[]} allowed @returns {Record<string, unknown>} */
function patternRecord(value, allowed) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
      || Reflect.ownKeys(value).some(key => typeof key !== 'string' || !allowed.includes(key))) {
    throw Object.assign(new Error('Invalid matcher pattern object or unsupported field'), { code: 'SPARS_INVALID_PATTERN' });
  }
  return Object.fromEntries(Object.entries(value));
}

/** @param {unknown} value @returns {unknown[]} */
function patternArray(value) {
  if (!Array.isArray(value)) throw Object.assign(new Error('Expected a matcher pattern array'), { code: 'SPARS_INVALID_PATTERN' });
  return value;
}

/** @param {unknown} value @param {'token' | 'dependency'} kind */
function validatePatterns(value, kind) {
  const field = kind === 'token' ? 'tokens' : 'nodes';
  for (const valuePattern of patternArray(value)) {
    const pattern = patternRecord(valuePattern, [field]);
    for (const valueItem of patternArray(pattern[field])) {
      const item = patternRecord(valueItem, kind === 'token' ? ['constraints', 'repetition'] : ['id', 'constraints', 'link']);
      for (const valueConstraint of patternArray(item.constraints)) {
        const constraint = patternRecord(valueConstraint, ['attribute', 'predicate']);
        patternRecord(constraint.predicate, ['kind', 'operator', 'value', 'values']);
      }
      if (kind === 'token') patternRecord(item.repetition, ['kind', 'min', 'max']);
      else if (item.link != null) patternRecord(item.link, ['left', 'relation']);
    }
  }
}

module.exports = { ERROR_CODES, OFFICIAL_MODEL_NAMES, Scheduler, configureExecution, configureInputLimits, install, installEntityRuler, installMatcher, isOfficialModelName, isSparsError };
