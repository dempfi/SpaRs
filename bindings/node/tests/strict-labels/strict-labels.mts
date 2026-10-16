// Compiled separately (tsconfig.json here) because augmentation is global to a compilation. The
// package name resolves to the local declarations, as it does for an installed package.
import type { DependencyLabel, Document, EntityInput, EntityLabel, EntityMatch, EntityPattern, EntityRuler, RulerPattern, FineGrainedTag, ModelName, ModelNames, SpanGroupKey, SpanGroupKeys, SpanInput, SpanLabel, SpanPattern, SpanRuler, SpanRulerPattern, Token } from '@spars/node';

declare module '@spars/node' {
  interface EntityLabels { PRODUCT_CODE: true }
  interface FineGrainedTags { CUSTOM_TAG: true }
  interface ModelNames { my_model: true }
  interface SpanLabels { CANDIDATE: true }
  interface SpanGroupKeys { candidates: true }
  interface StrictLabelSets { entity: true; tag: true; dependency: true; span: true }
}

type Assert<T extends true> = T;
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;

export type Contracts = [
  // Registered labels are accepted, and strict label sets reject unregistered ones.
  Assert<Equal<'PRODUCT_CODE' extends EntityLabel ? true : false, true>>,
  Assert<Equal<'PERSON' extends EntityLabel ? true : false, true>>,
  Assert<Equal<'UNREGISTERED' extends EntityLabel ? true : false, false>>,
  Assert<Equal<string extends NonNullable<Token['entityType']> ? true : false, false>>,
  Assert<Equal<string extends NonNullable<Document['entities']>[number]['label'] ? true : false, false>>,
  Assert<Equal<string extends EntityInput['label'] ? true : false, false>>,
  Assert<Equal<string extends EntityPattern['label'] ? true : false, false>>,
  Assert<Equal<string extends EntityMatch['label'] ? true : false, false>>,
  Assert<Equal<string extends RulerPattern['label'] ? true : false, false>>,
  Assert<Equal<string extends EntityRuler['labels'][number] ? true : false, false>>,
  Assert<Equal<'CUSTOM_TAG' extends FineGrainedTag ? true : false, true>>,
  Assert<Equal<'OTHER_TAG' extends FineGrainedTag ? true : false, false>>,
  Assert<Equal<'nsubj' extends DependencyLabel ? true : false, true>>,
  Assert<Equal<'custom_dep' extends DependencyLabel ? true : false, false>>,
  // Span labels include registered span and entity labels; strict sets reject others.
  Assert<Equal<'CANDIDATE' extends SpanLabel ? true : false, true>>,
  Assert<Equal<'PRODUCT_CODE' extends SpanLabel ? true : false, true>>,
  Assert<Equal<'UNREGISTERED' extends SpanLabel ? true : false, false>>,
  Assert<Equal<string extends NonNullable<SpanInput['label']> ? true : false, false>>,
  Assert<Equal<string extends NonNullable<Document['spanGroups']>[number]['spans'][number]['label'] ? true : false, false>>,
  Assert<Equal<'CANDIDATE' extends SpanPattern['label'] ? true : false, true>>,
  Assert<Equal<string extends SpanPattern['label'] ? true : false, false>>,
  Assert<Equal<string extends SpanRulerPattern['label'] ? true : false, false>>,
  Assert<Equal<string extends SpanRuler['labels'][number] ? true : false, false>>,
  Assert<Equal<string extends Awaited<ReturnType<SpanRuler['findMatches']>>[number]['label'] ? true : false, false>>,
  Assert<Equal<'candidates' extends NonNullable<SpanRuler['spansKey']> ? true : false, true>>,
  // Span group keys autocomplete registered keys but accept any key.
  Assert<Equal<'candidates' extends keyof SpanGroupKeys ? true : false, true>>,
  Assert<Equal<'other' extends SpanGroupKey ? true : false, true>>,
  Assert<Equal<'candidates' extends SpanGroupKey ? true : false, true>>,
  // Model names autocomplete registered names but always accept a path.
  Assert<Equal<'my_model' extends keyof ModelNames ? true : false, true>>,
  Assert<Equal<'./models/custom' extends ModelName ? true : false, true>>,
];

export function label(value: EntityLabel): string { return value; }
label('PRODUCT_CODE');
// @ts-expect-error strict entity labels reject unregistered values
label('UNREGISTERED');

// Strict span labels also apply to rule changes and lookups.
export function strictRuler(ruler: SpanRuler): void {
  ruler.remove('CANDIDATE');
  // @ts-expect-error an unregistered span label
  ruler.remove('UNREGISTERED');
  void ruler.contains('CANDIDATE');
  // @ts-expect-error an unregistered span label
  void ruler.contains('UNREGISTERED');
}
