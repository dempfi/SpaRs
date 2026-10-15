"""Freeze spaCy EntityRuler matches, entity annotation and pattern lifecycle."""
import argparse
from collections.abc import Sequence
from dataclasses import asdict, dataclass, field
import json
from pathlib import Path
import random
from typing import Literal, NotRequired, TypedDict
import warnings

import spacy
from spacy.pipeline import EntityRuler
from spacy.tokens import Doc as make_doc
from spacy.tokens import Span as make_span

from dependency_match_reference import Constraint, Equals, OfficialValue
from entity_update_reference import verify_source
from reference_types import Doc, Language, SpanRecord, span_records
from token_match_reference import Item, Pattern, Repetition, official, word

SOURCES = ['spacy/pipeline/entityruler.py', 'spacy/pipeline/factories.py', 'spacy/matcher/matcher.pyx',
           'spacy/matcher/phrasematcher.pyx', 'spacy/tokens/doc.pyx', 'spacy/tokens/span.pyx']
type PhraseAttribute = Literal['ORTH', 'LOWER', 'LEMMA']


class OfficialPattern(TypedDict):
    """An `EntityRuler.add_patterns` entry."""
    label: str
    pattern: str | list[dict[str, OfficialValue]]
    id: NotRequired[str]


@dataclass(frozen=True)
class RulerToken:
    """A token as SpaRs stores it: missing entity annotation is None, and so is the empty ID."""
    start: int
    end: int
    idx: int
    whitespace: bool
    norm: str
    tag: str
    pos: str
    morphology: str
    lemma: str
    head: int
    dep: str
    sentence_start: bool | None
    entity_iob: str | None
    entity_type: str | None
    entity_id: str | None


@dataclass(frozen=True)
class EntityRecord:
    start: int
    end: int
    label: str
    id: str | None


@dataclass(frozen=True)
class Document:
    text: str
    tokens: list[RulerToken]
    # None where spaCy has no such annotation.
    entities: list[EntityRecord] | None
    sentences: list[SpanRecord] | None
    noun_chunks: list[SpanRecord] | None


@dataclass(frozen=True)
class RulerPattern:
    """One `add_patterns` entry: a token pattern, or a phrase recorded as the pattern document spaCy builds."""
    label: str
    id: str | None
    tokens: Pattern | None = None
    phrase: Document | None = None


@dataclass(frozen=True)
class Options:
    overwrite_entities: bool = False
    phrase_attribute: PhraseAttribute = 'ORTH'
    id_separator: str = '||'


@dataclass(frozen=True)
class Step:
    action: Literal['add', 'remove', 'clear', 'apply']
    patterns: list[RulerPattern] = field(default_factory=list[RulerPattern])
    id: str | None = None
    # Apply to the case's document, or to the result of the previous application.
    input: Literal['initial', 'previous'] | None = None


@dataclass(frozen=True)
class ListedPattern:
    """One entry of `EntityRuler.patterns`."""
    label: str
    id: str | None
    tokens: Pattern | None
    phrase: str | None


@dataclass(frozen=True)
class EntityMatch:
    label: str
    id: str | None
    start: int
    end: int


@dataclass(frozen=True)
class Outcome:
    error: str | None
    length: int
    labels: list[str]
    ids: list[str]
    patterns: list[ListedPattern]
    matches: list[EntityMatch] | None
    result: Document | None


@dataclass(frozen=True)
class Case:
    id: str
    options: Options
    document: Document
    steps: list[Step]
    outcomes: list[Outcome]


@dataclass(frozen=True)
class Fixture:
    format_version: int
    versions: dict[str, str]
    sources: dict[str, str]
    cases: list[Case]


class AmbiguousTie(ValueError):
    """Identical spans from different rules, ordered by Python set iteration in spaCy."""


def optional_spans(read: 'SpanReader', doc: Doc) -> list[SpanRecord] | None:
    try:
        return read(doc)
    except ValueError:
        return None


class SpanReader:
    def __init__(self, kind: Literal['sentences', 'noun_chunks']) -> None:
        self.kind: Literal['sentences', 'noun_chunks'] = kind

    def __call__(self, doc: Doc) -> list[SpanRecord]:
        return span_records(doc.sents if self.kind == 'sentences' else doc.noun_chunks)


def document(doc: Doc) -> Document:
    text = doc.text
    tokens = [RulerToken(
        len(text[:t.idx].encode()), len(text[:t.idx + len(t)].encode()), t.idx, bool(t.whitespace_), t.norm_, t.tag_,
        t.pos_, str(t.morph), t.lemma_, t.head.i, t.dep_, t.is_sent_start, t.ent_iob_ or None,
        t.ent_type_ if t.ent_iob_ else None, t.ent_id_ or None) for t in doc]
    entities = ([EntityRecord(e.start, e.end, e.label_, e.id_ or None) for e in doc.ents]
                if doc.has_annotation('ENT_IOB') else None)
    noun_chunks = optional_spans(SpanReader('noun_chunks'), doc) if doc.has_annotation('DEP') else None
    return Document(text, tokens, entities, optional_spans(SpanReader('sentences'), doc), noun_chunks)


def lower(text: str) -> Item:
    return Item([Constraint('lower', Equals(text))])


def token_pattern(label: str, items: list[Item], id: str | None = None) -> RulerPattern:
    return RulerPattern(label, id, tokens=Pattern(items))


class Runner:
    """Apply steps to an EntityRuler added at the end of a pipeline, as `nlp.add_pipe` does."""

    def __init__(self, nlp: Language, name: str, options: Options) -> None:
        self.nlp = nlp
        self.name = name
        config: dict[str, str | bool | None] = {
            'overwrite_ents': options.overwrite_entities, 'ent_id_sep': options.id_separator,
            'phrase_matcher_attr': None if options.phrase_attribute == 'ORTH' else options.phrase_attribute}
        ruler = nlp.add_pipe('entity_ruler', name, config=config)
        if not isinstance(ruler, EntityRuler):
            raise TypeError('entity_ruler did not create an EntityRuler')
        self.ruler = ruler
        self.token_patterns: dict[str, Pattern] = {}

    def phrase(self, label: str, text: str, id: str | None = None) -> RulerPattern:
        """Record the pattern document spaCy makes: the text processed by the components before the ruler."""
        with self.nlp.select_pipes(disable=[self.name]):
            return RulerPattern(label, id, phrase=document(self.nlp(text)))

    def official(self, pattern: RulerPattern) -> OfficialPattern:
        value: str | list[dict[str, OfficialValue]]
        if pattern.tokens is not None:
            value = official(pattern.tokens)
            self.token_patterns[json.dumps(value, sort_keys=True)] = pattern.tokens
        elif pattern.phrase is not None:
            value = pattern.phrase.text
        else:
            raise ValueError('A pattern needs tokens or a phrase')
        entry: OfficialPattern = {'label': pattern.label, 'pattern': value}
        if pattern.id is not None:
            entry['id'] = pattern.id
        return entry

    def listed(self) -> list[ListedPattern]:
        result: list[ListedPattern] = []
        for entry in self.ruler.patterns:
            value = entry['pattern']
            id = entry.get('id')
            if isinstance(value, str):
                result.append(ListedPattern(entry['label'], id, None, value))
            else:
                result.append(ListedPattern(entry['label'], id, self.token_patterns[json.dumps(value, sort_keys=True)], None))
        return result

    def matches(self, doc: Doc) -> list[EntityMatch]:
        keyed = self.ruler.match(doc)
        spans = [(start, end) for _, start, end in keyed]
        if len(spans) != len(set(spans)):
            raise AmbiguousTie('identical spans from different rules')
        result: list[EntityMatch] = []
        for key, start, end in keyed:
            if key in self.ruler._ent_ids:
                label, id = self.ruler._ent_ids[key]
                result.append(EntityMatch(label, id or None, start, end))
            else:
                result.append(EntityMatch(self.nlp.vocab.strings[key], None, start, end))
        return result

    def run(self, initial: Doc, steps: list[Step]) -> list[Outcome]:
        outcomes: list[Outcome] = []
        previous = initial
        try:
            for step in steps:
                error: str | None = None
                found: list[EntityMatch] | None = None
                result: Document | None = None
                if step.action == 'add':
                    self.ruler.add_patterns([self.official(p) for p in step.patterns])
                elif step.action == 'remove':
                    if step.id is None:
                        raise ValueError('remove needs an ID')
                    try:
                        self.ruler.remove(step.id)
                    except ValueError as exc:
                        error = type(exc).__name__
                elif step.action == 'clear':
                    self.ruler.clear()
                else:
                    doc = (initial if step.input == 'initial' else previous).copy()
                    found = self.matches(doc)
                    previous = self.ruler(doc)
                    result = document(previous)
                outcomes.append(Outcome(error, len(self.ruler), list(self.ruler.labels), sorted(self.ruler.ent_ids),
                                        self.listed(), found, result))
        finally:
            self.nlp.remove_pipe(self.name)
        return outcomes


def blank_doc(nlp: Language, words: Sequence[str], entities: list[tuple[int, int, str, str | None]] | None = None, *,
              blocked: list[tuple[int, int]] | None = None, outside: list[tuple[int, int]] | None = None,
              spaces: list[bool] | None = None) -> Doc:
    """A tokenized document; given entity annotation is set with `set_ents`, leaving other tokens missing."""
    doc = make_doc(nlp.vocab, words=list(words), spaces=spaces if spaces is not None else [True] * len(words))
    if entities is not None or blocked or outside:
        spans = [make_span(doc, s, e, label=label, span_id=id or '') for s, e, label, id in entities or []]
        doc.set_ents(spans, blocked=[make_span(doc, s, e) for s, e in blocked or []],
                     outside=[make_span(doc, s, e) for s, e in outside or []], default='missing')
    return doc


class BuildSteps:
    """Cases build phrase patterns with the runner that will apply them."""

    def __init__(self, nlp: Language, case_id: str, options: Options = Options()) -> None:
        self.nlp = nlp
        self.case_id = case_id
        self.options = options
        self.runner = Runner(nlp, f'ruler_{case_id}', options)

    def phrase(self, label: str, text: str, id: str | None = None) -> RulerPattern:
        return self.runner.phrase(label, text, id)

    def capture(self, doc: Doc, steps: list[Step]) -> Case:
        return Case(self.case_id, self.options, document(doc), steps, self.runner.run(doc, steps))


def add(*patterns: RulerPattern) -> Step:
    return Step('add', list(patterns))


def apply(input: Literal['initial', 'previous'] = 'initial') -> Step:
    return Step('apply', input=input)


def remove(id: str) -> Step:
    return Step('remove', id=id)


CLEAR = Step('clear')


def upstream_patterns(case: BuildSteps) -> list[RulerPattern]:
    """The `patterns` fixture of spaCy's test_entity_ruler.py."""
    return [
        case.phrase('HELLO', 'hello world'),
        token_pattern('BYE', [lower('bye'), lower('bye')]),
        token_pattern('HELLO', [word('HELLO')]),
        token_pattern('COMPLEX', [word('foo', Repetition('zero_or_more'))]),
        case.phrase('TECH_ORG', 'Apple', 'a1'),
        case.phrase('TECH_ORG', 'Microsoft', 'a2'),
    ]


def authored(nlp: Language) -> list[Case]:
    cases: list[Case] = []
    def doc(text: str) -> Doc:
        return blank_doc(nlp, text.split())
    for name, overwrite in [('existing', False), ('existing-overwrite', True)]:
        case = BuildSteps(nlp, name, Options(overwrite_entities=overwrite))
        cases.append(case.capture(blank_doc(nlp, 'OH HELLO WORLD bye bye'.split(), [(0, 3, 'ORG', None)]),
                                  [add(*upstream_patterns(case)), apply()]))
    case = BuildSteps(nlp, 'existing-complex', Options(overwrite_entities=True))
    cases.append(case.capture(blank_doc(nlp, 'foo foo bye bye'.split(), [(0, 3, 'ORG', None)]),
                              [add(*upstream_patterns(case)), apply()]))
    case = BuildSteps(nlp, 'pattern-ids', Options(overwrite_entities=True))
    cases.append(case.capture(doc('Apple is a technology company and Microsoft too'), [add(*upstream_patterns(case)), apply()]))
    case = BuildSteps(nlp, 'overlapping-spans')
    cases.append(case.capture(doc('foo bar baz'), [add(case.phrase('FOOBAR', 'foo bar'), case.phrase('BARBAZ', 'bar baz')), apply()]))
    case = BuildSteps(nlp, 'same-id-different-patterns')
    cases.append(case.capture(doc('San Francisco San Fran Apple'), [add(
        case.phrase('ORG', 'Apple'), token_pattern('GPE', [lower('san'), lower('francisco')], 'san-francisco'),
        token_pattern('GPE', [lower('san'), lower('fran')], 'san-francisco')), apply()]))
    case = BuildSteps(nlp, 'lower-phrases-with-ids', Options(phrase_attribute='LOWER'))
    cases.append(case.capture(nlp.make_doc('Democratic front-runner Joe Biden. Sen. Bernie Sanders joined in.'), [add(
        case.phrase('PERSON', 'joe biden', 'joe-biden'), case.phrase('PERSON', 'bernie sanders', 'bernie-sanders')), apply()]))
    case = BuildSteps(nlp, 'custom-id-separator', Options(id_separator='**'))
    cases.append(case.capture(doc('x y z w'), [
        add(case.phrase('A||B', 'x'), case.phrase('A', 'y', 'B')), apply(),
        add(case.phrase('C**D', 'z'), case.phrase('C', 'w', 'D')), apply()]))
    # The default separator joins a label and ID into one rule key, so a label containing the
    # separator and a label with an ID can share a key.
    case = BuildSteps(nlp, 'separator-in-label')
    cases.append(case.capture(doc('x y z'), [add(case.phrase('A||B', 'x')), apply(), add(case.phrase('A', 'y', 'B')), apply(),
                                              add(case.phrase('A||B', 'z', 'C')), apply(), remove('B'), apply()]))
    # With overwriting, an existing entity overlapping a match is removed whole; its other tokens
    # become outside and keep their IDs, which a new entity without an ID then reports.
    for name, overwrite in [('partial-overlap-overwrite', True), ('partial-overlap-kept', False)]:
        case = BuildSteps(nlp, name, Options(overwrite_entities=overwrite))
        cases.append(case.capture(blank_doc(nlp, list('abcdef'), [(1, 4, 'X', 'old'), (5, 6, 'Z', None)]),
                                  [add(case.phrase('Y', 'c d e')), apply()]))
    # Blocked tokens have no entity type, so matches on them are kept; the setter makes every
    # token not in an entity outside, including missing and blocked ones.
    case = BuildSteps(nlp, 'blocked-and-missing')
    cases.append(case.capture(blank_doc(nlp, list('abcdef'), [], blocked=[(0, 2)], outside=[(5, 6)]),
                              [add(case.phrase('Y', 'a b'), case.phrase('W', 'e')), apply()]))
    case = BuildSteps(nlp, 'no-patterns')
    cases.append(case.capture(blank_doc(nlp, list('abcd'), [(1, 2, 'X', 'i')], blocked=[(2, 3)]), [apply()]))
    case = BuildSteps(nlp, 'no-entity-annotation')
    cases.append(case.capture(doc('a b c'), [apply(), add(case.phrase('Y', 'b')), apply()]))
    case = BuildSteps(nlp, 'empty-matches-and-patterns')
    cases.append(case.capture(doc('a b a'), [add(
        token_pattern('OPT', [word('z', Repetition('optional'))]), token_pattern('STAR', [word('z', Repetition('zero_or_more'))]),
        case.phrase('EMPTY', ''), token_pattern('A', [word('a'), word('z', Repetition('zero_or_more'))])), apply()]))
    case = BuildSteps(nlp, 'duplicate-patterns')
    cases.append(case.capture(doc('a b c'), [add(
        case.phrase('X', 'a b'), case.phrase('X', 'a b'), token_pattern('X', [word('a'), word('b')]),
        token_pattern('X', [word('a'), word('b')]), case.phrase('Y', 'c', 'i'), token_pattern('Y', [word('c')], 'i')), apply()]))
    # Longer matches win, then earlier ones; adjacent matches are both kept.
    case = BuildSteps(nlp, 'priority')
    cases.append(case.capture(doc('a b c d e f g'), [add(
        case.phrase('SHORT', 'b'), case.phrase('LONG', 'a b c'), case.phrase('LATER', 'c d e'), case.phrase('EQUAL', 'd e f'),
        case.phrase('ADJACENT', 'g'), token_pattern('GREEDY', [word('e'), word('f', Repetition('optional'))])), apply()]))
    case = BuildSteps(nlp, 'token-ids')
    cases.append(case.capture(blank_doc(nlp, list('abcdef'), [(0, 2, 'X', 'a'), (3, 4, 'Z', 'z')], outside=[(4, 6)]), [
        add(token_pattern('Y', [word('d')]), token_pattern('E', [word('e')], ''), case.phrase('F', 'f', '')), apply(),
        remove('')]))
    case = BuildSteps(nlp, 'lifecycle')
    cases.append(case.capture(doc('Apple met Tim in Paris'), [
        add(case.phrase('ORG', 'Apple', 'a'), token_pattern('GPE', [word('Paris')], 'a'), case.phrase('PERSON', 'Tim', 'b')),
        apply(), remove('a'), apply(), remove('a'), remove('missing'), CLEAR, apply(),
        add(case.phrase('ORG', 'Apple', 'a')), apply()]))
    # Rule names removed by `remove` keep their label and ID, so a later label containing the
    # separator inherits them; `clear` forgets them.
    case = BuildSteps(nlp, 'stale-id-after-remove')
    cases.append(case.capture(doc('x y'), [add(case.phrase('A', 'x', 'B')), remove('B'), add(case.phrase('A||B', 'y')), apply(),
                                            remove('B')]))
    case = BuildSteps(nlp, 'clear-resets-ids')
    cases.append(case.capture(doc('x y'), [add(case.phrase('A', 'x', 'B')), CLEAR, add(case.phrase('A||B', 'x')), apply(),
                                            remove('B')]))
    # Within one addition spaCy records token patterns' IDs before phrases', so when two label
    # and ID pairs form the same rule name the phrase's pair wins.
    case = BuildSteps(nlp, 'separator-collision')
    cases.append(case.capture(doc('x y'), [add(case.phrase('A||B', 'x', 'C'), token_pattern('A', [word('y')], 'B||C')), apply(),
                                            remove('B||C')]))
    case = BuildSteps(nlp, 'remove-and-add')
    cases.append(case.capture(doc('Dina went to school'), [
        add(case.phrase('PERSON', 'Dina', 'dina')), apply(), remove('dina'), apply(), add(case.phrase('PERSON', 'Dina', 'dina')),
        apply(), remove('dina'), apply()]))
    case = BuildSteps(nlp, 'same-id-two-labels')
    cases.append(case.capture(doc('Dina founded DinaCorp and ACME .'), [add(
        case.phrase('PERSON', 'Dina', 'dina'), case.phrase('ORG', 'DinaCorp', 'dina'), case.phrase('ORG', 'ACME', 'acme')),
        apply(), remove('dina'), apply()]))
    case = BuildSteps(nlp, 'clear-lower', Options(phrase_attribute='LOWER'))
    cases.append(case.capture(doc('Joe Biden met JOE BIDEN'), [add(case.phrase('PERSON', 'joe biden')), apply(), CLEAR, apply(),
                                                               add(case.phrase('PERSON', 'joe biden')), apply()]))
    case = BuildSteps(nlp, 'empty-phrase-with-id')
    cases.append(case.capture(doc('a'), [add(case.phrase('E', '', 'e')), remove('e'), remove('e')]))
    # One accepted match removes two existing entities; a dropped match leaves the entity it
    # overlaps.
    case = BuildSteps(nlp, 'overwrite-several', Options(overwrite_entities=True))
    cases.append(case.capture(blank_doc(nlp, list('abcdef'), [(0, 2, 'X', 'x'), (2, 4, 'Y', None), (5, 6, 'Z', None)]),
                              [add(case.phrase('B', 'b c'), case.phrase('D', 'd e'), case.phrase('F', 'e f')), apply()]))
    # spaCy accepts an empty label. Its matches claim tokens and remove overwritten entities, but
    # setting the entities drops them.
    case = BuildSteps(nlp, 'empty-label', Options(overwrite_entities=True))
    cases.append(case.capture(blank_doc(nlp, 'w x y z'.split(), [(0, 2, 'E', 'e')]), [
        add(case.phrase('', 'x'), token_pattern('', [word('z')], 'q'), case.phrase('W', 'x y')), apply(), remove('q')]))
    case = BuildSteps(nlp, 'apply-again')
    cases.append(case.capture(blank_doc(nlp, list('abcd'), [(0, 1, 'X', None)]), [
        add(case.phrase('Y', 'b c', 'y')), apply(), apply('previous')]))
    case = BuildSteps(nlp, 'empty-document')
    cases.append(case.capture(make_doc(nlp.vocab, words=[]), [add(case.phrase('Y', 'a')), apply()]))
    case = BuildSteps(nlp, 'unicode-and-spacing')
    cases.append(case.capture(nlp.make_doc('Café  naïve 東京 ok'), [add(case.phrase('X', 'naïve 東京'), token_pattern('Y', [word('Café')])), apply()]))
    return cases


def model_cases(nlp: Language) -> list[Case]:
    text = 'Apple is looking at buying a U.K. startup for $1 billion. Tim Cook visited London.'
    cases: list[Case] = []
    # Documents are processed before a case adds its ruler to the pipeline.
    predicted = nlp(text)
    for name, overwrite in [('after-ner', False), ('after-ner-overwrite', True)]:
        case = BuildSteps(nlp, name, Options(overwrite_entities=overwrite))
        doc = predicted
        cases.append(case.capture(doc, [add(
            case.phrase('PERSON', 'Tim Cook', 'tim-cook'), token_pattern('ORG', [lower('apple')], 'apple'),
            case.phrase('ORG', 'U.K. startup'), case.phrase('MONEY', '$1')), apply()]))
    lemmas = nlp('She buys apples and he bought pears.')
    case = BuildSteps(nlp, 'lemma-phrases', Options(phrase_attribute='LEMMA'))
    cases.append(case.capture(lemmas, [add(case.phrase('ACT', 'buy'), case.phrase('FRUIT', 'apple')), apply()]))
    with nlp.select_pipes(disable=['ner']):
        doc = nlp(text)
    case = BuildSteps(nlp, 'without-ner')
    cases.append(case.capture(doc, [add(case.phrase('PERSON', 'Tim Cook', 'tim-cook')), apply()]))
    return cases


def random_case(nlp: Language, rng: random.Random, index: int) -> Case:
    words = [rng.choice('abcd') for _ in range(8)]
    used: set[int] = set()
    def take() -> tuple[int, int] | None:
        start = rng.randrange(8)
        span = (start, min(8, start + rng.randrange(1, 4)))
        if used.intersection(range(*span)):
            return None
        used.update(range(*span))
        return span
    entities: list[tuple[int, int, str, str | None]] = []
    for _ in range(rng.randrange(0, 3)):
        span = take()
        if span is not None:
            entities.append((*span, rng.choice(['X', 'Y']), rng.choice(['e', None])))
    blocked = [s for s in [take() if rng.random() < 0.3 else None] if s is not None]
    outside = [s for s in [take() if rng.random() < 0.3 else None] if s is not None]
    annotated = rng.random() < 0.8
    doc = blank_doc(nlp, words, entities if annotated else None, blocked=blocked if annotated else None,
                    outside=outside if annotated else None)
    case = BuildSteps(nlp, f'random-{index}', Options(overwrite_entities=rng.random() < 0.5))
    patterns: list[RulerPattern] = []
    for _ in range(rng.randrange(1, 6)):
        label, id = rng.choice(['P', 'Q', 'R']), rng.choice([None, 'p', 'q'])
        if rng.random() < 0.5:
            patterns.append(case.phrase(label, ' '.join(rng.choice('abcd') for _ in range(rng.randrange(1, 4))), id))
        else:
            items = [word(rng.choice('abcd'), Repetition(rng.choice(['once', 'once', 'optional', 'zero_or_more', 'one_or_more'])))
                     for _ in range(rng.randrange(1, 4))]
            patterns.append(token_pattern(label, items, id))
    return case.capture(doc, [add(*patterns), apply()])


def build() -> Fixture:
    versions, sources = verify_source(SOURCES)
    warnings.filterwarnings('ignore', message=r'\[W036\]')
    model = spacy.load('en_core_web_md')
    if model.meta.get('version') != '3.8.0':
        raise ValueError('Expected en_core_web_md 3.8.0')
    versions['en_core_web_md'] = '3.8.0'
    cases = authored(spacy.blank('en')) + model_cases(model)
    rng = random.Random(2021)
    nlp = spacy.blank('en')
    while len(cases) < 61:
        try:
            cases.append(random_case(nlp, rng, len(cases)))
        except AmbiguousTie:
            # spaCy orders identical spans from different rules by Python set iteration, which SpaRs
            # does not reproduce; see docs/SPACY_DIFFERENCES.md. Such cases are drawn again.
            continue
    return Fixture(1, versions, sources, cases)


class Arguments(argparse.Namespace):
    output: Path = Path('target/reports/entity-ruler.json')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    arguments = Arguments()
    parser.parse_args(namespace=arguments)
    if arguments.output.exists():
        raise ValueError('Refusing to overwrite frozen output')
    fixture = build()
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(asdict(fixture), ensure_ascii=False, indent=2) + '\n')
    outcomes = [o for c in fixture.cases for o in c.outcomes]
    applied = [o for o in outcomes if o.result is not None]
    print(f'{len(fixture.cases)} cases; {len(outcomes)} steps; {len(applied)} applications; '
          f'{sum(len(o.matches or []) for o in applied)} matches; {sum(o.error is not None for o in outcomes)} errors')


if __name__ == '__main__':
    main()
