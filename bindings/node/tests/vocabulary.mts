import assert from 'node:assert/strict';
import type {
  Comparison, DependencyRelation, EntityIob, FlagAttribute, IntegerSetKind, OfficialModelName, PhraseAttribute, SpanFilter,
  SparsErrorCode, StringAttribute, StringSetKind, TokenAttribute, TokenConstraint, TokenRepetition, UniversalPos,
} from '../index.js';
import { array, record } from './fixtures.mts';

type Assert<T extends true> = T;
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;

// Runtime copies of the declared unions. Each `Exact` check fails to compile if a list misses or
// adds a member, so the runtime tests below exercise every declared value.
export const PHRASE_ATTRIBUTES = ['ORTH', 'TEXT', 'LOWER', 'NORM', 'LEMMA', 'POS', 'TAG', 'DEP', 'MORPH', 'IS_ALPHA', 'IS_ASCII',
  'IS_DIGIT', 'IS_LOWER', 'IS_UPPER', 'IS_TITLE', 'IS_PUNCT', 'IS_SPACE', 'IS_BRACKET', 'IS_QUOTE', 'IS_LEFT_PUNCT',
  'IS_RIGHT_PUNCT', 'IS_CURRENCY', 'IS_STOP', 'LIKE_NUM', 'LIKE_URL', 'LIKE_EMAIL', 'LENGTH'] as const;
export const STRING_ATTRIBUTES = ['text', 'lower', 'norm', 'lemma'] as const;
export const FLAG_ATTRIBUTES = ['is_alpha', 'is_digit', 'is_space', 'is_punct', 'like_num', 'is_lower', 'is_upper', 'is_title',
  'is_ascii', 'is_currency', 'is_stop', 'is_bracket', 'is_quote', 'is_left_punct', 'is_right_punct', 'like_url', 'like_email'] as const;
export const TOKEN_ATTRIBUTES = [...STRING_ATTRIBUTES, 'pos', 'tag', 'dep', 'morphology', ...FLAG_ATTRIBUTES, 'length'] as const;
export const COMPARISONS = ['==', '!=', '>=', '<=', '>', '<'] as const;
export const STRING_SET_KINDS = ['in', 'not_in', 'is_subset', 'is_superset', 'intersects'] as const;
export const INTEGER_SET_KINDS = ['in_integers', 'not_in_integers', 'is_subset_integers', 'is_superset_integers', 'intersects_integers'] as const;
export const REPETITIONS = ['once', 'optional', 'zero_or_more', 'one_or_more', 'negated'] as const;
export const RELATIONS = ['<', '>', '<<', '>>', '.', '.*', ';', ';*', '$+', '$-', '$++', '$--', '>+', '>-', '>++', '>--',
  '<+', '<-', '<++', '<--'] as const;
export const UNIVERSAL_POS = ['ADJ', 'ADP', 'ADV', 'AUX', 'CCONJ', 'CONJ', 'DET', 'EOL', 'INTJ', 'NOUN', 'NUM', 'PART', 'PRON',
  'PROPN', 'PUNCT', 'SCONJ', 'SPACE', 'SYM', 'VERB', 'X'] as const;
export const ENTITY_IOB = ['B', 'I', 'O'] as const;
export const ERROR_CODES = ['SPARS_IO', 'SPARS_INVALID_MODEL', 'SPARS_UNSUPPORTED', 'SPARS_INVALID_TEXT', 'SPARS_TEXT_TOO_LONG',
  'SPARS_BOUNDS', 'SPARS_INFERENCE', 'SPARS_INVALID_PATTERN', 'SPARS_BUSY', 'SPARS_INPUT_LIMIT', 'SPARS_NATIVE_INCOMPATIBLE'] as const;
export const OFFICIAL_MODEL_NAMES = ['en_core_web_sm', 'en_core_web_md', 'en_core_web_lg'] as const;
export const SPAN_FILTERS = ['first_longest', 'prioritize_new', 'prioritize_existing'] as const;

export type Exact = [
  Assert<Equal<(typeof PHRASE_ATTRIBUTES)[number], PhraseAttribute>>,
  Assert<Equal<(typeof STRING_ATTRIBUTES)[number], StringAttribute>>,
  Assert<Equal<(typeof FLAG_ATTRIBUTES)[number], FlagAttribute>>,
  Assert<Equal<(typeof TOKEN_ATTRIBUTES)[number], TokenAttribute>>,
  Assert<Equal<(typeof COMPARISONS)[number], Comparison>>,
  Assert<Equal<(typeof STRING_SET_KINDS)[number], StringSetKind>>,
  Assert<Equal<(typeof INTEGER_SET_KINDS)[number], IntegerSetKind>>,
  Assert<Equal<(typeof REPETITIONS)[number] | 'range', TokenRepetition['kind']>>,
  Assert<Equal<(typeof RELATIONS)[number], DependencyRelation>>,
  Assert<Equal<(typeof UNIVERSAL_POS)[number], UniversalPos>>,
  Assert<Equal<(typeof ENTITY_IOB)[number], EntityIob>>,
  Assert<Equal<(typeof ERROR_CODES)[number], SparsErrorCode>>,
  Assert<Equal<(typeof OFFICIAL_MODEL_NAMES)[number], OfficialModelName>>,
  Assert<Equal<(typeof SPAN_FILTERS)[number], SpanFilter>>,
  Assert<Equal<TokenConstraint['attribute'], TokenAttribute>>,
  Assert<Equal<TokenConstraint['predicate']['kind'],
    'equals' | 'flag' | 'compare' | 'morph_superset' | 'morph_intersects' | StringSetKind | IntegerSetKind>>,
];

/** Narrow a value to a member of a literal list, failing the test otherwise. */
export function member<T extends string>(list: readonly T[], value: unknown): T {
  const found = list.find(item => item === value);
  assert.ok(found !== undefined, `${String(value)} is not one of ${list.join(', ')}`);
  return found;
}
function includes<T extends string>(list: readonly T[], value: string): value is T {
  return list.some(item => item === value);
}
function string(value: unknown): string { assert.equal(typeof value, 'string'); return String(value); }
function boolean(value: unknown): boolean { assert.equal(typeof value, 'boolean'); return value === true; }
function finite(value: unknown): number { assert.ok(typeof value === 'number' && Number.isFinite(value)); return value; }
function integer(value: unknown): number { assert.ok(typeof value === 'number' && Number.isSafeInteger(value)); return value; }

/**
 * A POS condition whose values are outside the universal POS tags. spaCy accepts such a pattern, but
 * such a value never equals a token's POS (under `not_in` it excludes nothing), so the declared types
 * reject it; official fixtures record a few deliberately.
 */
export interface OutOfVocabularyPos {
  attribute: 'pos';
  predicate: { kind: 'equals'; value: string } | { kind: StringSetKind; values: Array<string> };
}
/** A recorded pattern: typed when it fits the declarations, otherwise kept for an untyped call. */
export type Recorded<Pattern> = { typed: true; pattern: Pattern } | { typed: false; pattern: unknown };

/** Parse a recorded condition into the declared union, so fixtures also check the types. */
export function recordedConstraint(value: unknown): { typed: true; constraint: TokenConstraint } | { typed: false; constraint: OutOfVocabularyPos } {
  const data = record(value);
  const predicate = record(data.predicate);
  if (data.attribute === 'pos') {
    const values = predicate.kind === 'equals' ? [predicate.value] : array(predicate.values);
    if (!values.every(item => item === '' || includes(UNIVERSAL_POS, string(item)))) {
      const kind = string(predicate.kind);
      return { typed: false, constraint: { attribute: 'pos', predicate: kind === 'equals'
        ? { kind, value: string(predicate.value) }
        : { kind: member(STRING_SET_KINDS, kind), values: array(predicate.values).map(string) } } };
    }
  }
  return { typed: true, constraint: constraint(value) };
}

/** Parse a recorded condition that must fit the declared union. */
export function constraint(value: unknown): TokenConstraint {
  const data = record(value);
  const predicate = record(data.predicate);
  const attribute = member(TOKEN_ATTRIBUTES, data.attribute);
  const kind = string(predicate.kind);
  if (attribute === 'length') {
    if (kind === 'compare') return { attribute, predicate: { kind, operator: member(COMPARISONS, predicate.operator), value: finite(predicate.value) } };
    return { attribute, predicate: { kind: member(INTEGER_SET_KINDS, kind), values: array(predicate.values).map(integer) } };
  }
  if (includes(FLAG_ATTRIBUTES, attribute)) {
    assert.equal(kind, 'flag');
    return { attribute, predicate: { kind: 'flag', value: boolean(predicate.value) } };
  }
  if (attribute === 'morphology' && (kind === 'morph_superset' || kind === 'morph_intersects')) {
    return { attribute, predicate: { kind, values: array(predicate.values).map(string) } };
  }
  const values = (parse: (value: unknown) => string) => kind === 'equals'
    ? { kind, value: parse(predicate.value) } as const
    : { kind: member(STRING_SET_KINDS, kind), values: array(predicate.values).map(parse) };
  if (attribute === 'pos') {
    const pos = (item: unknown): UniversalPos | '' => item === '' ? '' : member(UNIVERSAL_POS, item);
    return { attribute, predicate: kind === 'equals'
      ? { kind, value: pos(predicate.value) }
      : { kind: member(STRING_SET_KINDS, kind), values: array(predicate.values).map(pos) } };
  }
  return { attribute, predicate: values(string) };
}

export function repetition(value: unknown): TokenRepetition {
  const data = record(value);
  if (data.kind === 'range') {
    return data.max === undefined || data.max === null
      ? { kind: 'range', min: integer(data.min) }
      : { kind: 'range', min: integer(data.min), max: integer(data.max) };
  }
  assert.equal(data.min ?? undefined, undefined);
  assert.equal(data.max ?? undefined, undefined);
  return { kind: member(REPETITIONS, data.kind) };
}

export function relation(value: unknown): DependencyRelation {
  return member(RELATIONS, value);
}
