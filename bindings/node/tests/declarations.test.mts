import assert from 'node:assert/strict';
import { test } from 'node:test';
import { restrictConstructor, addEntityPatternTypes, addSpanGroupTypes, addSpanRulerTypes, addMatcherLoaders, addExecutionLoader, addExecutionTypes, addVocabularyTypes, replaceInterface } from '../scripts/declarations.mts';

test('declaration correction preserves methods and rejects unexpected generator changes', () => {
  const source = 'export declare class Model {\n  process(text: string): Promise<Document>\n}';
  const expected = source.replace('{', '{\n  private constructor()');
  assert.equal(restrictConstructor(source), expected);
  assert.equal(restrictConstructor(expected), expected);
  for (const bad of ['', source + source, 'export declare class Model { constructor() }']) {
    assert.throws(() => restrictConstructor(bad));
  }
});

test('execution wrapper is generated once and rejects changed loader contracts', () => {
  const loader = 'module.exports.Model = nativeBinding.Model';
  const wrapped = addExecutionLoader(loader);
  assert.match(wrapped, /execution\.cjs/);
  assert.match(wrapped, /module.exports.configureExecution/);
  assert.match(wrapped, /module.exports.configureInputLimits/);
  assert.equal(addExecutionLoader(wrapped), wrapped);
  assert.throws(() => addExecutionLoader(''), /Expected native Model/);
  const declaration = addExecutionTypes('');
  assert.match(declaration, /configureExecution/);
  assert.match(declaration, /configureInputLimits/);
  assert.equal(addExecutionTypes(declaration), declaration);
});

test('matcher and ruler wrappers are installed once each', () => {
  const loader = addMatcherLoaders('');
  for (const name of ['PhraseMatcher', 'TokenMatcher', 'DependencyMatcher', 'EntityRuler', 'SpanRuler']) {
    assert.equal(loader.split(`module.exports.${name},`).length - 1, 1, name);
  }
  assert.match(loader, /installEntityRuler\(module\.exports\.EntityRuler, module\.exports\.NativeDocument\)/);
  assert.match(loader, /installSpanRuler\(module\.exports\.SpanRuler, module\.exports\.NativeDocument\)/);
  assert.equal(loader.split('installDocument(module.exports.NativeDocument)').length - 1, 1);
  assert.equal(addMatcherLoaders(loader), loader);
});

test('span group types are appended once', () => {
  const declaration = addSpanGroupTypes('');
  assert.match(declaration, /export type SpanGroupAttrs = /);
  assert.match(declaration, /export interface SpanGroupInput/);
  assert.match(declaration, /export type JsonInput = /);
  assert.equal(addSpanGroupTypes(declaration), declaration);
});

test('entity pattern types are appended once', () => {
  const declaration = addEntityPatternTypes('');
  assert.match(declaration, /export type EntityPattern = EntityTokenPattern \| EntityPhrasePattern/);
  assert.equal(addEntityPatternTypes(declaration), declaration);
});

test('span ruler types are appended once', () => {
  const declaration = addSpanRulerTypes('');
  assert.match(declaration, /export type SpanFilter = 'first_longest' \| 'prioritize_new' \| 'prioritize_existing'/);
  assert.match(declaration, /export type SpanPattern = SpanTokenPattern \| SpanPhrasePattern/);
  assert.match(declaration, /export interface SpanRulerOptions/);
  assert.equal(addSpanRulerTypes(declaration), declaration);
});

test('interface replacement checks the generated fields and runs once', () => {
  const generated = 'export interface Span {\n  start: number\n  /** A label. */\n  label?: string\n}\nexport interface Other {\n  x: number\n}\n';
  const replaced = replaceInterface(generated, 'Span', ['start', 'label'], 'export interface Span<L> {\n  label: L\n}');
  assert.equal(replaced, 'export interface Span<L> {\n  label: L\n}\nexport interface Other {\n  x: number\n}\n');
  assert.equal(replaceInterface(replaced, 'Span', ['start', 'label'], 'unused'), replaced);
  assert.equal(replaceInterface(generated.replaceAll('\n', '\r\n'), 'Span', ['start', 'label'], ''), 'export interface Other {\r\n  x: number\r\n}\r\n');
  assert.throws(() => replaceInterface(generated, 'Span', ['start'], ''), /fields start, label; expected start/);
  assert.throws(() => replaceInterface(generated + generated, 'Span', ['start', 'label'], ''), /Expected one generated Span/);
});

test('vocabulary is appended once and stale vocabularies are rejected', () => {
  const vocabulary = "export type UniversalPos = 'NOUN'\n";
  const declarations = addVocabularyTypes('export interface A {}', vocabulary);
  assert.equal(addVocabularyTypes(declarations, vocabulary), declarations);
  assert.throws(() => addVocabularyTypes(declarations, "export type UniversalPos = 'VERB'\n"), /older vocabulary/);
  assert.throws(() => addVocabularyTypes('', 'export type Other = 1'), /Expected the vocabulary/);
});
