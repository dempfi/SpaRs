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

export function addExecutionTypes(source: string): string {
  return source.includes('export interface ExecutionOptions') ? source : source + executionTypes;
}

export function addExecutionLoader(source: string): string {
  const marker = "require('./execution.cjs').install(module.exports);";
  if (source.includes(marker)) return source;
  if (!source.includes('module.exports.Model = nativeBinding.Model')) throw new Error('Expected native Model export');
  return source + `\n${marker}\nmodule.exports.configureExecution = require('./execution.cjs').configureExecution;\nmodule.exports.configureInputLimits = require('./execution.cjs').configureInputLimits;\nmodule.exports.isSparsError = require('./execution.cjs').isSparsError;\nmodule.exports.isOfficialModelName = require('./execution.cjs').isOfficialModelName;\n`;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const path = new URL('../index.d.ts', import.meta.url);
  let declarations = readFileSync(path, 'utf8');
  for (const name of ['Model', 'NativeDocument', 'NativeToken', 'NativeSpan']) declarations = restrictConstructor(declarations, name);
  for (const { name, fields, replacement } of replacements) declarations = replaceInterface(declarations, name, fields, replacement);
  const vocabulary = readFileSync(new URL('./vocabulary.d.ts', import.meta.url), 'utf8');
  writeFileSync(path, addVocabularyTypes(addExecutionTypes(declarations), vocabulary));
  const loader = new URL('../index.js', import.meta.url);
  let source = addExecutionLoader(readFileSync(loader, 'utf8'));
  for (const name of ['PhraseMatcher', 'TokenMatcher', 'DependencyMatcher']) {
    const hook = `require('./execution.cjs').installMatcher(module.exports.${name}, module.exports.NativeDocument, '${name.replace('Matcher', '').toLowerCase()}');`;
    if (!source.includes(hook)) source += `\n${hook}\n`;
  }
  writeFileSync(loader, source);
}
