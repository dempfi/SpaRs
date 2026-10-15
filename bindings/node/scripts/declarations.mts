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
/** Inference admission limits shared by models in this package instance and JS isolate. */
export interface ExecutionOptions {
  maxActive: number
  maxQueued: number
}
/** Configure admission while idle. Defaults to 2 active and 32 queued; null restores these defaults. */
export declare function configureExecution(options: ExecutionOptions | null): void
/** Text lengths count UTF-16 units, matching JavaScript string.length. */
export interface InputLimits {
  maxTextLength: number
  maxBatchSize: number
  maxBatchTextLength: number
}
/** Enable additional server input limits while idle. Disabled by default; null resets them. */
export declare function configureInputLimits(options: InputLimits | null): void
export interface PipeOptions {
  batchSize?: number
  stage?: Stage | null
}
export interface Model {
  /** Lazily buffer and yield documents in order; native inference stays sequential within a batch. */
  pipe(texts: Iterable<string> | AsyncIterable<string>, options?: PipeOptions): AsyncGenerator<Document, void, unknown>
}
/** Whether a value is an error raised by this package, narrowing its \`code\`. */
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
  /** spaCy's \`Span.id_\`, or \`''\` without one; only entities carry IDs. */
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
   * Written to matched entities and their tokens, and the key for \`EntityRuler.remove\`, which removes the ID's patterns under every label. Without one, matched entities have \`id\` \`''\`.
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
  for (const hook of hooks) if (!source.includes(hook)) source += `\n${hook}\n`;
  return source;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const path = new URL('../index.d.ts', import.meta.url);
  let declarations = readFileSync(path, 'utf8');
  for (const name of ['Model', 'NativeDocument', 'NativeToken', 'NativeSpan']) declarations = restrictConstructor(declarations, name);
  for (const { name, fields, replacement } of replacements) declarations = replaceInterface(declarations, name, fields, replacement);
  const vocabulary = readFileSync(new URL('./vocabulary.d.ts', import.meta.url), 'utf8');
  writeFileSync(path, addVocabularyTypes(addEntityPatternTypes(addExecutionTypes(declarations)), vocabulary));
  const loader = new URL('../index.js', import.meta.url);
  let source = addExecutionLoader(readFileSync(loader, 'utf8'));
  source = addMatcherLoaders(source);
  writeFileSync(loader, source);
}
