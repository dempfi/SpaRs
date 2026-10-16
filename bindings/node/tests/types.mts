import { Model } from '../index.js';
import { downloadModel, EntityRuler, loadModel, PhraseMatcher, SpanRuler, TokenMatcher } from '../index.js';
import type { Document, Token, TokenAnnotations, Span, Stage, NativeDocument, Lexeme, ModelStore, PhraseMatch, TokenMatch, DependencyMatcher, DependencyMatch,
  DependencyLink, DependencyLabel, DependencyPattern, DependencyRelation, EntityIob, EntityLabel, EntityLabels, EntityInput, FineGrainedTag, ModelName,
  PhraseAttribute, SparsError, SparsErrorCode, TokenConstraint, TokenPattern, UniversalPos, EntityMatch, EntityPattern, RulerPattern,
  EntityRulerOptions, JsonInput, JsonValue, SpanGroup, SpanGroupAttrs, SpanGroupInput, SpanGroupKey, SpanInput, SpanLabel,
  SpanFilter, SpanPattern, SpanRulerOptions, SpanRulerPattern } from '../index.js';
import type { ByteOffset, CodePointOffset, Utf16Offset, TokenIndex } from '../units.js';

type Assert<T extends true> = T;
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Distinct<A, B> = A extends B ? false : true;

export type Contracts = [
  // Span groups: precise output, typed inputs, and keys and labels that autocomplete known values.
  Assert<Equal<Document['spanGroups'], SpanGroup[]>>,
  Assert<Equal<SpanGroup['key'], SpanGroupKey>>,
  Assert<Equal<SpanGroup['attrs'], SpanGroupAttrs>>,
  Assert<Equal<SpanGroup['spans'], Span<SpanLabel | ''>[]>>,
  Assert<Equal<SpanGroupAttrs[string], JsonValue>>,
  Assert<Equal<SpanGroupInput['spans'], SpanInput[]>>,
  Assert<Equal<ReturnType<NativeDocument['withSpanGroup']>, NativeDocument>>,
  Assert<Equal<ReturnType<NativeDocument['withoutSpanGroup']>, NativeDocument>>,
  Assert<Equal<Parameters<NativeDocument['withSpanGroup']>, [key: SpanGroupKey, group: SpanGroupInput]>>,
  Assert<Equal<'ruler' extends SpanGroupKey ? true : false, true>>,
  Assert<Equal<'sc' extends SpanGroupKey ? true : false, true>>,
  Assert<Equal<'custom' extends SpanGroupKey ? true : false, true>>,
  Assert<Equal<'PERSON' extends SpanLabel ? true : false, true>>,
  Assert<Equal<'ANY_LABEL' extends SpanLabel ? true : false, true>>,
  Assert<Equal<undefined extends SpanGroupAttrs[string] ? true : false, false>>,
  Assert<Equal<Extract<SpanGroupKey, 'ruler' | 'sc'>, 'ruler' | 'sc'>>,
  Assert<Equal<Equal<SpanGroupKey, string>, false>>,
  Assert<Equal<JsonValue, null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }>>,
  Assert<Equal<Date extends JsonValue ? true : false, false>>,
  Assert<Equal<bigint extends JsonValue ? true : false, false>>,
  Assert<Equal<(() => void) extends JsonValue ? true : false, false>>,
  Assert<Equal<Parameters<NativeDocument['withoutSpanGroup']>, [key: SpanGroupKey]>>,
  Assert<Equal<SpanGroupInput['name'], string | null | undefined>>,
  Assert<Equal<SpanGroupInput['attrs'], { readonly [key: string]: JsonInput } | null | undefined>>,
  Assert<Equal<(readonly [1, 'a']) extends JsonInput ? true : false, true>>,
  Assert<Equal<Date extends JsonInput ? true : false, false>>,
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
  Assert<Equal<ReturnType<EntityRuler['findMatches']>, Promise<EntityMatch[]>>>,
  Assert<Equal<ReturnType<EntityRuler['annotate']>, Promise<NativeDocument>>>,
  Assert<Equal<Parameters<EntityRuler['add']>[0], EntityPattern[]>>,
  Assert<Equal<EntityRuler['patterns'], RulerPattern[]>>,
  Assert<Equal<EntityRuler['labels'], (EntityLabel | '')[]>>,
  Assert<Equal<EntityRuler['phraseAttribute'], PhraseAttribute>>,
  Assert<Equal<EntityRulerOptions['phraseAttribute'], PhraseAttribute | null | undefined>>,
  Assert<Equal<EntityRulerOptions['overwriteEntities'], boolean | null | undefined>>,
  Assert<Equal<EntityRulerOptions['idSeparator'], string | null | undefined>>,
  Assert<Equal<EntityRuler['ids'], string[]>>,
  Assert<Equal<EntityRuler['size'], number>>,
  Assert<Equal<EntityRuler['overwriteEntities'], boolean>>,
  Assert<Equal<EntityRuler['idSeparator'], string>>,
  Assert<Equal<RulerPattern['id'], string | undefined>>,
  Assert<Equal<Extract<RulerPattern, { tokens: TokenPattern }>['phrase'], undefined>>,
  Assert<Equal<EntityMatch['end'], TokenIndex>>,
  Assert<Equal<ConstructorParameters<typeof EntityRuler>, [options?: EntityRulerOptions | null, model?: Model | null]>>,
  Assert<Equal<EntityPattern['label'], EntityLabel | ''>>,
  Assert<Equal<EntityPattern['id'], string | undefined>>,
  Assert<Equal<EntityMatch['label'], EntityLabel | ''>>,
  Assert<Equal<EntityMatch['id'], string>>,
  Assert<Equal<EntityMatch['start'], TokenIndex>>,
  Assert<Equal<RulerPattern['label'], EntityLabel | ''>>,
  Assert<Equal<Extract<RulerPattern, { phrase: string }>['tokens'], undefined>>,
  // SpanRuler: spaCy's filter names, a nullable span group key, and patterns listed with their exact IDs.
  Assert<Equal<SpanFilter, 'first_longest' | 'prioritize_new' | 'prioritize_existing'>>,
  Assert<Equal<ConstructorParameters<typeof SpanRuler>, [options?: SpanRulerOptions | null, model?: Model | null]>>,
  Assert<Equal<SpanRulerOptions['spansKey'], SpanGroupKey | null | undefined>>,
  Assert<Equal<SpanRulerOptions['spansFilter'], SpanFilter | null | undefined>>,
  Assert<Equal<SpanRulerOptions['entitiesFilter'], SpanFilter | null | undefined>>,
  Assert<Equal<SpanRulerOptions['annotateEntities'], boolean | null | undefined>>,
  Assert<Equal<SpanRulerOptions['overwrite'], boolean | null | undefined>>,
  Assert<Equal<SpanRulerOptions['phraseAttribute'], PhraseAttribute | null | undefined>>,
  Assert<Equal<SpanRuler['spansKey'], SpanGroupKey | null>>,
  Assert<Equal<SpanRuler['spansFilter'], SpanFilter | null>>,
  Assert<Equal<SpanRuler['entitiesFilter'], SpanFilter>>,
  Assert<Equal<SpanRuler['annotateEntities'], boolean>>,
  Assert<Equal<SpanRuler['overwrite'], boolean>>,
  Assert<Equal<SpanRuler['phraseAttribute'], PhraseAttribute>>,
  Assert<Equal<SpanRuler['size'], number>>,
  Assert<Equal<SpanRuler['labels'], (SpanLabel | '')[]>>,
  Assert<Equal<SpanRuler['ids'], string[]>>,
  Assert<Equal<SpanRuler['patterns'], SpanRulerPattern[]>>,
  Assert<Equal<Parameters<SpanRuler['add']>[0], SpanPattern[]>>,
  Assert<Equal<Parameters<SpanRuler['remove']>[0], SpanLabel | ''>>,
  Assert<Equal<Parameters<SpanRuler['contains']>[0], SpanLabel | ''>>,
  Assert<Equal<Parameters<SpanRuler['removeById']>[0], string>>,
  Assert<Equal<ReturnType<SpanRuler['findMatches']>, Promise<Span<SpanLabel | ''>[]>>>,
  Assert<Equal<ReturnType<SpanRuler['annotate']>, Promise<NativeDocument>>>,
  Assert<Equal<SpanPattern['label'], SpanLabel | ''>>,
  Assert<Equal<SpanPattern['id'], string | undefined>>,
  Assert<Equal<SpanRulerPattern['label'], SpanLabel | ''>>,
  Assert<Equal<SpanRulerPattern['id'], string | undefined>>,
  Assert<Equal<Extract<SpanRulerPattern, { tokens: TokenPattern }>['phrase'], undefined>>,
  Assert<Equal<Extract<SpanRulerPattern, { phrase: string }>['tokens'], undefined>>,
];

// Closed sets reject typos at compile time. Each rejected line follows an accepted line of the same
// shape, so an unrelated error (such as a changed method signature) cannot satisfy the expectation.
export function rejected(link: DependencyLink, constraint: TokenConstraint, document: NativeDocument): void {
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
  const ruler = new EntityRuler({ phraseAttribute: 'LOWER' });
  // @ts-expect-error phrase attribute names are uppercase
  void new EntityRuler({ phraseAttribute: 'lower' });
  ruler.add([{ label: 'ORG', tokens: { tokens: [] } }]);
  // @ts-expect-error a pattern has token conditions or a phrase, not both
  ruler.add([{ label: 'ORG', tokens: { tokens: [] }, phrase: document }]);
  // @ts-expect-error a pattern needs token conditions or a phrase
  ruler.add([{ label: 'ORG' }]);
  // @ts-expect-error a phrase is a processed document, not text
  ruler.add([{ label: 'ORG', phrase: 'Acme' }]);
  const spans = new SpanRuler({ spansKey: null, spansFilter: 'prioritize_new', entitiesFilter: 'first_longest' });
  // @ts-expect-error filter names are spaCy's snake case names
  void new SpanRuler({ spansFilter: 'prioritizeNew' });
  // @ts-expect-error filter names omit spaCy's registry prefix
  void new SpanRuler({ entitiesFilter: 'spacy.first_longest_spans_filter.v1' });
  spans.add([{ label: 'ORG', id: '', phrase: document }]);
  // @ts-expect-error a span pattern has token conditions or a phrase, not both
  spans.add([{ label: 'ORG', tokens: { tokens: [] }, phrase: document }]);
  // @ts-expect-error a span phrase is a processed document, not text
  spans.add([{ label: 'ORG', phrase: 'Acme' }]);
  void downloadModel('en_core_web_lg');
  // @ts-expect-error downloadModel installs only official pipelines
  void downloadModel('en_core_web_xl');
  void constraint;
}
