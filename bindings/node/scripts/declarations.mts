import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

/** napi-rs omits a constructor declaration for classes created by worker tasks. */
export function restrictConstructor(source: string, name = 'Model'): string {
  const declaration = new RegExp(`export declare class ${name} \\{([^}]*)\\}`, 'g');
  const matches = [...source.matchAll(declaration)];
  const match = matches[0];
  if (matches.length !== 1 || !match || match[1] === undefined) throw new Error(`Expected one generated ${name} class`);
  if (match[1].includes('private constructor()')) return source;
  if (match[1].includes('constructor(')) throw new Error(`Review changed native ${name} constructor`);
  return source.replace(`export declare class ${name} {`, `export declare class ${name} {\n  private constructor()`);
}

const executionTypes = `
/**
 * Limits on concurrent native work for every model, matcher and entity ruler in this JavaScript
 * thread. They cover \`process\`, \`processDocument\`, \`processBatch\`, \`pipe\`, \`findMatches\` and
 * \`EntityRuler.annotate\`, but not \`loadModel\` or \`downloadModel\`.
 */
export interface ExecutionOptions {
  /** Calls running on Node's worker pool at once; a positive integer. */
  maxActive: number
  /** Further calls that wait in order once \`maxActive\` are running; 0 or more. When this many are waiting, new calls reject with \`SPARS_BUSY\`. */
  maxQueued: number
}
/**
 * Configure execution limits. Defaults to 2 active and 32 queued; null restores these defaults.
 * Throws a plain \`Error\` while any call is running or queued, and \`RangeError\` for invalid values.
 */
export declare function configureExecution(options: ExecutionOptions | null): void
/**
 * Optional caps for untrusted input, checked before native work starts; exceeding one rejects with
 * \`SPARS_INPUT_LIMIT\`. Lengths count UTF-16 units (\`string.length\`). All fields are positive integers.
 */
export interface InputLimits {
  /** Per text in \`process\`, \`processDocument\` and \`processBatch\`, and per document (\`utf16Length\`) in \`findMatches\` and \`EntityRuler.annotate\`. */
  maxTextLength: number
  /** Texts per \`processBatch\` call. */
  maxBatchSize: number
  /** Total length of the texts in one \`processBatch\` call. */
  maxBatchTextLength: number
}
/**
 * Set input limits in addition to \`Model.maxLength\`. Disabled by default; null removes them. Throws a
 * plain \`Error\` while any call is running or queued.
 */
export declare function configureInputLimits(options: InputLimits | null): void
export interface PipeOptions {
  /** Texts per \`processBatch\` call; defaults to \`model.batchSize\`. */
  batchSize?: number
  /** Defaults to the full pipeline (through \`Ner\`). */
  stage?: Stage | null
}
export interface Model {
  /** Process an iterable of texts lazily, yielding documents in input order. Each \`batchSize\` texts go to one \`processBatch\` call, so its errors and limits apply. */
  pipe(texts: Iterable<string> | AsyncIterable<string>, options?: PipeOptions): AsyncGenerator<Document, void, unknown>
}
/**
 * Whether \`error\` is an \`Error\` whose \`code\` is a \`SparsErrorCode\`, narrowing its \`code\`. Argument
 * errors (\`TypeError\`, \`RangeError\`, or Node-API codes such as \`InvalidArg\`) and the plain \`Error\`
 * thrown when changing settings during inference return false.
 */
export declare function isSparsError(error: unknown): error is SparsError
/** Whether a name is an official pipeline that \`downloadModel\` can install. */
export declare function isOfficialModelName(name: string): name is OfficialModelName
`;

/**
 * napi-rs cannot express these shapes: a generic span label and the discriminated unions that tie each
 * token attribute to its predicates. Replace the generated interfaces with the precise types.
 */
/** Generated interfaces replaced by precise declarations, with the fields the generator must emit. */
const replacements: ReadonlyArray<{ name: string; fields: readonly string[]; replacement: string }> = [
  { name: 'Span', fields: ['start', 'end', 'label', 'id'], replacement: `export interface Span<Label extends string = string, Id extends string = string> {
  start: import('./units.js').TokenIndex
  end: import('./units.js').TokenIndex
  label: Label
  /** spaCy's \`Span.id_\`, or \`''\` without one; entities and span group members carry IDs. */
  id: Id
}` },
  { name: 'RulerPattern', fields: ['label', 'id', 'tokens', 'phrase'], replacement: `/** A pattern as \`EntityRuler.patterns\` lists it, like an entry of spaCy's \`EntityRuler.patterns\`. */
export type RulerPattern = RulerTokenPattern | RulerPhrasePattern
interface RulerPatternBase {
  /** Read back from the rule name, so a label containing the ID separator is listed split at its last separator. */
  label: EntityLabel | ''
  /** Absent for a pattern without an ID or with an empty one. */
  id?: string
}
/** A token pattern as listed by \`EntityRuler.patterns\`. */
export interface RulerTokenPattern extends RulerPatternBase {
  tokens: TokenPattern
  phrase?: never
}
/** A phrase as listed by \`EntityRuler.patterns\`. */
export interface RulerPhrasePattern extends RulerPatternBase {
  /** The full text of the phrase's pattern document, including whitespace. */
  phrase: string
  tokens?: never
}` },
  { name: 'TokenConstraint', fields: ['attribute', 'predicate'], replacement: '' },
  { name: 'TokenPredicate', fields: ['kind', 'operator', 'value', 'values'], replacement: '' },
  { name: 'TokenRepetition', fields: ['kind', 'min', 'max'], replacement: '' },
];

/**
 * Replace one generated interface. It fails if the generator emits the interface more than once or
 * with different fields, so a new native field cannot be dropped silently. An absent interface was
 * already replaced.
 */
export function replaceInterface(source: string, name: string, fields: readonly string[], replacement: string): string {
  const declaration = new RegExp(`export interface ${name} \\{[^}]*\\}\\r?\\n`, 'g');
  const matches = [...source.matchAll(declaration)];
  if (matches.length === 0 && !source.includes(`export interface ${name} {`)) return source;
  if (matches.length !== 1) throw new Error(`Expected one generated ${name} interface`);
  const generated = [...matches[0]![0].matchAll(/^\s*(\w+)\??:/gm)].map(field => field[1]);
  if (generated.join() !== fields.join()) {
    throw new Error(`Generated ${name} has fields ${generated.join(', ')}; expected ${fields.join(', ')}`);
  }
  return source.replace(declaration, replacement ? `${replacement}\n` : '');
}

/** Append the vocabulary types once; they are the targets of the overrides above. */
export function addVocabularyTypes(source: string, vocabulary: string): string {
  if (!vocabulary.includes('export type UniversalPos')) throw new Error('Expected the vocabulary declarations');
  if (source.includes(vocabulary)) return source;
  if (source.includes('export type UniversalPos')) throw new Error('Declarations contain an older vocabulary; regenerate them');
  return `${source}\n${vocabulary}`;
}

const entityPatternTypes = `
/** Settings fixed when an \`EntityRuler\` is created, named after spaCy's \`entity_ruler\` config. Other properties, such as spaCy's own \`overwrite_ents\`, and values of the wrong type throw \`InvalidArg\`. */
export interface EntityRulerOptions {
  /**
   * spaCy's \`overwrite_ents\`, default \`false\`. When \`false\`, a match is skipped if any of its tokens already has an entity type; blocked tokens have none, so they do not stop a match.
   * When \`true\`, every existing entity an accepted match overlaps is removed whole.
   */
  overwriteEntities?: boolean | null
  /**
   * spaCy's \`phrase_matcher_attr\`: the token value phrase patterns compare, default \`'ORTH'\` (exact token text). Token patterns are unaffected.
   * \`LEMMA\`, \`POS\`, \`TAG\`, \`DEP\` and \`MORPH\` need phrase documents carrying that annotation. A lexical flag such as \`IS_ALPHA\` needs a ruler created with a model; without one, every phrase \`add\` throws \`SPARS_INVALID_PATTERN\`.
   * Names are uppercase; spaCy also accepts lowercase.
   */
  phraseAttribute?: PhraseAttribute | null
  /**
   * spaCy's \`ent_id_sep\`, default \`'||'\`; an empty separator throws \`SPARS_INVALID_PATTERN\`. A pattern with an ID is stored under the rule name \`label + idSeparator + id\`,
   * and \`labels\`, \`ids\` and \`patterns\` split rule names at the last separator, as spaCy does, so a label or ID containing the separator can be listed split differently from how it was added,
   * and two label and ID pairs that join to the same rule name share one rule.
   */
  idSeparator?: string | null
}
/** A pattern for \`EntityRuler.add\`, like an entry of spaCy's \`add_patterns\`: token conditions or a phrase. */
export type EntityPattern = EntityTokenPattern | EntityPhrasePattern
interface EntityPatternBase {
  /** The label of matched entities. An empty label is accepted, as in spaCy: its matches claim their tokens and remove overwritten entities, but add no entity. */
  label: EntityLabel | ''
  /**
   * Written to the entities this pattern's accepted matches create and to their tokens' \`entityId\`, and the key for \`EntityRuler.remove\`, which removes the ID's patterns under every label. Without one, as in spaCy, each matched token keeps its current \`entityId\`, so the entity reports its first token's earlier ID, which is \`''\` unless \`withEntities\` or an earlier \`annotate\` set one.
   * On a phrase, \`''\` is the same as no ID. On a token pattern, \`''\` also gives matches no ID, but as in spaCy \`EntityRuler.ids\` lists \`''\` and \`remove('')\` removes the pattern.
   */
  id?: string
}
/** An \`EntityRuler.add\` pattern of token conditions. */
export interface EntityTokenPattern extends EntityPatternBase {
  /** Token conditions, matched as by \`TokenMatcher\`. Lexical flag conditions need a ruler created with a model; otherwise \`add\` throws \`SPARS_INVALID_PATTERN\`. */
  tokens: TokenPattern
  phrase?: never
}
/** An \`EntityRuler.add\` pattern that matches a phrase document. */
export interface EntityPhrasePattern extends EntityPatternBase {
  /**
   * A document processed by the same model stages as the documents the ruler annotates, as spaCy runs phrase patterns through the components before the ruler.
   * Only the ruler's \`phraseAttribute\` is compared. For LEMMA, POS, TAG, DEP and MORPH, a nonempty document in which no token has that annotation throws \`SPARS_INVALID_PATTERN\`. An empty document is accepted and counted by \`size\` but never matches. The ruler copies what it needs, so the document can be discarded afterwards.
   */
  phrase: NativeDocument
  tokens?: never
}
`;

const spanGroupTypes = `
/** A JSON value: what span group attrs can hold. */
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }
/** A JSON value given as input, which may be read-only, such as a value declared \`as const\`. */
export type JsonInput = null | boolean | number | string | readonly JsonInput[] | { readonly [key: string]: JsonInput }
/**
 * Span group attributes, like spaCy's \`SpanGroup.attrs\`. Keys keep their order, except that JavaScript lists integer-like keys such as \`'1'\` first.
 * Numbers are JavaScript numbers, so integers beyond \`Number.MAX_SAFE_INTEGER\` in a snapshot lose precision when read.
 */
export type SpanGroupAttrs = { [key: string]: JsonValue }
/** A span group for \`NativeDocument.withSpanGroup\`, like a \`SpanGroup\` assigned to spaCy's \`Doc.spans\`. Unknown string-keyed properties on the group or on any span throw \`InvalidArg\`. */
export interface SpanGroupInput {
  /** spaCy's \`SpanGroup.name\`. Omitted or \`null\` uses the key, as when spaCy is given a plain list of spans; \`''\` is kept as an empty name. */
  name?: string | null
  /**
   * spaCy's \`SpanGroup.attrs\`. Omitted, \`null\` or \`{}\` means none, read back as \`{}\`.
   * Plain JSON only: \`null\`, booleans, finite numbers, strings, arrays without holes, and objects whose prototype is \`Object.prototype\` or \`null\`, with enumerable data properties.
   * Anything else, such as \`undefined\`, \`NaN\`, a \`Date\`, a getter or a circular reference, throws \`TypeError\`; a string or key with an unpaired surrogate throws \`SPARS_INVALID_TEXT\`.
   */
  attrs?: { readonly [key: string]: JsonInput } | null
  /** Kept in order; they may overlap, repeat or be empty. */
  spans: SpanInput[]
}
`;

export function addSpanGroupTypes(source: string): string {
  return source.includes('export type SpanGroupAttrs ') ? source : source + spanGroupTypes;
}

export function addEntityPatternTypes(source: string): string {
  return source.includes('export type EntityPattern ') ? source : source + entityPatternTypes;
}

export function addExecutionTypes(source: string): string {
  return source.includes('export interface ExecutionOptions') ? source : source + executionTypes;
}

export function addExecutionLoader(source: string): string {
  const marker = "require('./execution.cjs').install(module.exports);";
  if (source.includes(marker)) return source;
  if (!source.includes('module.exports.Model = nativeBinding.Model')) throw new Error('Expected native Model export');
  return source + `\n${marker}\nmodule.exports.configureExecution = require('./execution.cjs').configureExecution;\nmodule.exports.configureInputLimits = require('./execution.cjs').configureInputLimits;\nmodule.exports.isSparsError = require('./execution.cjs').isSparsError;\nmodule.exports.isOfficialModelName = require('./execution.cjs').isOfficialModelName;\n`;
}

/** Append the wrappers that queue searches and guard rule changes, once each. */
export function addMatcherLoaders(source: string): string {
  const hooks = ['PhraseMatcher', 'TokenMatcher', 'DependencyMatcher'].map(name =>
    `require('./execution.cjs').installMatcher(module.exports.${name}, module.exports.NativeDocument, '${name.replace('Matcher', '').toLowerCase()}');`);
  hooks.push("require('./execution.cjs').installEntityRuler(module.exports.EntityRuler, module.exports.NativeDocument);");
  hooks.push("require('./execution.cjs').installDocument(module.exports.NativeDocument);");
  for (const hook of hooks) if (!source.includes(hook)) source += `\n${hook}\n`;
  return source;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const path = new URL('../index.d.ts', import.meta.url);
  let declarations = readFileSync(path, 'utf8');
  for (const name of ['Model', 'NativeDocument', 'NativeToken', 'NativeSpan']) declarations = restrictConstructor(declarations, name);
  for (const { name, fields, replacement } of replacements) declarations = replaceInterface(declarations, name, fields, replacement);
  const vocabulary = readFileSync(new URL('./vocabulary.d.ts', import.meta.url), 'utf8');
  writeFileSync(path, addVocabularyTypes(addSpanGroupTypes(addEntityPatternTypes(addExecutionTypes(declarations))), vocabulary));
  const loader = new URL('../index.js', import.meta.url);
  let source = addExecutionLoader(readFileSync(loader, 'utf8'));
  source = addMatcherLoaders(source);
  writeFileSync(loader, source);
}
