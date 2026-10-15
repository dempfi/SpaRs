import { Model } from '../index.js';
import { downloadModel, loadModel, PhraseMatcher, TokenMatcher } from '../index.js';
import type { Document, Token, TokenAnnotations, Span, Stage, NativeDocument, Lexeme, ModelStore, PhraseMatch, TokenMatch, DependencyMatcher, DependencyMatch,
  DependencyLink, DependencyLabel, DependencyPattern, DependencyRelation, EntityIob, EntityLabel, EntityLabels, EntityInput, FineGrainedTag, ModelName,
  PhraseAttribute, SparsError, SparsErrorCode, TokenConstraint, TokenPattern, UniversalPos } from '../index.js';
import type { ByteOffset, CodePointOffset, Utf16Offset, TokenIndex } from '../units.js';

type Assert<T extends true> = T;
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Distinct<A, B> = A extends B ? false : true;

export type Contracts = [
  Assert<Distinct<typeof Model, new () => Model>>,
  Assert<Equal<Awaited<ReturnType<Model['process']>>, Document>>,
  Assert<Equal<ReturnType<Model['processDocument']>, Promise<NativeDocument>>>,
  Assert<Equal<ReturnType<PhraseMatcher['findMatches']>, Promise<PhraseMatch[]>>>,
  Assert<Equal<ReturnType<TokenMatcher['findMatches']>, Promise<TokenMatch[]>>>,
  Assert<Equal<ReturnType<DependencyMatcher['findMatches']>, Promise<DependencyMatch[]>>>,
  Assert<Equal<ReturnType<ModelStore['resolve']>, Promise<string>>>,
  Assert<Equal<ReturnType<ModelStore['register']>, Promise<void>>>,
  Assert<Equal<ReturnType<Model['lexeme']>, Lexeme>>,
  Assert<Equal<Lexeme['orth'], bigint>>,
  Assert<Equal<ReturnType<Model['documentVector']>, Float32Array>>,
  Assert<Equal<ReturnType<Model['spanVector']>, Float32Array>>,
  Assert<Equal<ReturnType<Model['tokenVector']>, Float32Array | null>>,
  Assert<Equal<ReturnType<Model['similarity']>, number>>,
  Assert<Equal<ReturnType<Model['spanSimilarity']>, number>>,
  Assert<Equal<Awaited<ReturnType<Model['processBatch']>>, Document[]>>,
  Assert<Equal<Document['entities'], Span<EntityLabel>[] | null>>,
  Assert<Equal<Document['sentences'], Span<'', ''>[] | null>>,
  Assert<Equal<Document['nounChunks'], Span<'NP', ''>[] | null>>,
  Assert<Equal<NonNullable<Document['entities']>[number]['id'], string>>,
  Assert<Equal<Token['whitespace'], '' | ' '>>,
  Assert<Equal<Token['pos'], UniversalPos | '' | null>>,
  Assert<Equal<Token['entityIob'], EntityIob | '' | null>>,
  Assert<Equal<Token['entityType'], EntityLabel | '' | null>>,
  Assert<Equal<Token['tag'], FineGrainedTag | '' | null>>,
  Assert<Equal<Token['dep'], DependencyLabel | '' | null>>,
  Assert<Equal<TokenAnnotations['entityIob'], Token['entityIob']>>,
  Assert<Equal<TokenAnnotations['pos'], Token['pos']>>,
  Assert<Equal<TokenAnnotations['tag'], Token['tag']>>,
  Assert<Equal<TokenAnnotations['dep'], Token['dep']>>,
  Assert<Equal<TokenAnnotations['entityType'], Token['entityType']>>,
  Assert<Equal<Parameters<typeof loadModel>[0], ModelName>>,
  Assert<Equal<Parameters<ModelStore['resolve']>[0], ModelName>>,
  Assert<Equal<Parameters<ModelStore['register']>[0], ModelName>>,
  Assert<Equal<SparsError['code'], SparsErrorCode>>,
  Assert<Equal<ReturnType<TokenMatcher['get']>, TokenPattern[] | null>>,
  Assert<Equal<ReturnType<DependencyMatcher['get']>, DependencyPattern[] | null>>,
  Assert<Equal<DependencyLink['relation'], DependencyRelation>>,
  Assert<Equal<EntityInput['label'], EntityLabel | ''>>,
  Assert<Equal<EntityInput['id'], string | undefined>>,
  Assert<Equal<Token['entityId'], string>>,
  Assert<Equal<TokenAnnotations['entityId'], string>>,
  Assert<Equal<Span['id'], string>>,
  Assert<Equal<PhraseMatcher['attribute'], PhraseAttribute>>,
  Assert<Equal<PhraseMatch['start'], TokenIndex>>,
  Assert<Equal<TokenMatch['end'], TokenIndex>>,
  Assert<Equal<Awaited<ReturnType<typeof downloadModel>>, string>>,
  // Known labels autocomplete, while other labels remain accepted by default.
  Assert<Equal<'PERSON' extends EntityLabel ? true : false, true>>,
  Assert<Equal<'PERSON' extends keyof EntityLabels ? true : false, true>>,
  Assert<Equal<'CUSTOM' extends EntityLabel ? true : false, true>>,
  Assert<Equal<'NNP' extends FineGrainedTag ? true : false, true>>,
  Assert<Equal<'nsubj' extends DependencyLabel ? true : false, true>>,
  Assert<Distinct<'noun', UniversalPos>>,
  Assert<Equal<Token['head'], TokenIndex | null>>,
  Assert<Equal<Token['byteStart'], ByteOffset>>,
  Assert<Equal<Token['codePointStart'], CodePointOffset>>,
  Assert<Equal<Token['utf16Start'], Utf16Offset>>,
  Assert<Distinct<ByteOffset, Utf16Offset>>,
  Assert<Distinct<CodePointOffset, TokenIndex>>,
  Assert<Distinct<'unknown', Stage>>,
  Assert<Equal<ReturnType<Model['vector']>, Float32Array | null>>,
];

// Closed sets reject typos at compile time. Each rejected line follows an accepted line of the same
// shape, so an unrelated error (such as a changed method signature) cannot satisfy the expectation.
export function rejected(link: DependencyLink, constraint: TokenConstraint): void {
  void new PhraseMatcher('LOWER');
  // @ts-expect-error an unknown attribute name
  void new PhraseMatcher('LOWERR');
  // @ts-expect-error attribute names are upper case
  void new PhraseMatcher('lower');
  link.relation = '>>';
  // @ts-expect-error relations are spaCy's operators
  link.relation = '>>>';
  constraint = { attribute: 'is_alpha', predicate: { kind: 'flag', value: true } };
  // @ts-expect-error flag attributes take only the flag predicate
  constraint = { attribute: 'is_alpha', predicate: { kind: 'equals', value: 'a' } };
  constraint = { attribute: 'length', predicate: { kind: 'in_integers', values: [1] } };
  // @ts-expect-error length takes numbers
  constraint = { attribute: 'length', predicate: { kind: 'in', values: ['1'] } };
  constraint = { attribute: 'pos', predicate: { kind: 'equals', value: 'NOUN' } };
  // @ts-expect-error pos values are universal POS tags
  constraint = { attribute: 'pos', predicate: { kind: 'equals', value: 'noun' } };
  constraint = { attribute: 'morphology', predicate: { kind: 'morph_superset', values: ['Number=Sing'] } };
  // @ts-expect-error morphology predicates apply only to morphology
  constraint = { attribute: 'lower', predicate: { kind: 'morph_superset', values: ['Number=Sing'] } };
  constraint = { attribute: 'length', predicate: { kind: 'compare', operator: '>', value: 1 } };
  // @ts-expect-error only length compares numbers
  constraint = { attribute: 'text', predicate: { kind: 'compare', operator: '>', value: 1 } };
  new TokenMatcher().add('r', [{ tokens: [{ constraints: [], repetition: { kind: 'range', min: 1 } }] }]);
  // @ts-expect-error only range repetitions take bounds
  new TokenMatcher().add('r', [{ tokens: [{ constraints: [], repetition: { kind: 'once', min: 1 } }] }]);
  // @ts-expect-error a range needs its minimum
  new TokenMatcher().add('r', [{ tokens: [{ constraints: [], repetition: { kind: 'range', max: 1 } }] }]);
  void downloadModel('en_core_web_lg');
  // @ts-expect-error downloadModel installs only official pipelines
  void downloadModel('en_core_web_xl');
  void constraint;
}
