"""Freeze spaCy SpanRuler matches, span groups, entity annotation and pattern lifecycle."""
import argparse
from collections.abc import Callable
from dataclasses import asdict, dataclass, field
import json
from pathlib import Path
import random
from typing import Literal
import warnings

import spacy
from spacy.pipeline.span_ruler import SpanRuler
from spacy.tokens import Doc as make_doc
from spacy.tokens import Span as make_span
from spacy.tokens import SpanGroup as make_group

from dependency_match_reference import OfficialValue
from entity_ruler_reference import EntityRecord, OfficialPattern, PhraseAttribute, RulerToken, blank_doc, document as entity_document, lower
from entity_update_reference import verify_source
from json_types import JsonValue
from reference_types import Doc, Language, SpanRecord as SentenceRecord
from span_group_reference import attrs
from token_match_reference import Item, Pattern, Repetition, official, word

SOURCES = ['spacy/pipeline/span_ruler.py', 'spacy/pipeline/factories.py', 'spacy/util.py', 'spacy/matcher/matcher.pyx',
           'spacy/matcher/phrasematcher.pyx', 'spacy/tokens/doc.pyx', 'spacy/tokens/span.pyx',
           'spacy/tokens/span_group.pyx', 'spacy/tokens/_dict_proxies.py']
type Filter = Literal['first_longest', 'prioritize_new', 'prioritize_existing']
FILTER_KINDS: list[Filter] = ['first_longest', 'prioritize_new', 'prioritize_existing']
FILTERS: dict[Filter, str] = {
    'first_longest': 'spacy.first_longest_spans_filter.v1',
    'prioritize_new': 'spacy.prioritize_new_ents_filter.v1',
    'prioritize_existing': 'spacy.prioritize_existing_ents_filter.v1',
}


@dataclass(frozen=True)
class SpanRecord:
    start: int
    end: int
    label: str
    # spaCy's Span.id_, with None for the empty ID.
    id: str | None


@dataclass(frozen=True)
class Group:
    key: str
    name: str
    attrs: dict[str, JsonValue]
    spans: list[SpanRecord]


@dataclass(frozen=True)
class Document:
    text: str
    tokens: list[RulerToken]
    # None where spaCy has no such annotation.
    entities: list[EntityRecord] | None
    sentences: list[SentenceRecord] | None
    noun_chunks: list[SentenceRecord] | None
    span_groups: list[Group]


@dataclass(frozen=True)
class RulerPattern:
    """One `add_patterns` entry: a token pattern, or a phrase recorded as the pattern document spaCy builds.

    `id` None leaves the entry without an `id` key, which spaCy treats like the empty ID for matching but not
    for `ids` or `remove_by_id`.
    """
    label: str
    id: str | None
    tokens: Pattern | None = None
    phrase: Document | None = None


@dataclass(frozen=True)
class Options:
    """The `span_ruler` factory settings, with the factory defaults."""
    spans_key: str | None = 'ruler'
    spans_filter: Filter | None = None
    annotate_entities: bool = False
    entities_filter: Filter = 'first_longest'
    phrase_attribute: PhraseAttribute = 'ORTH'
    overwrite: bool = True


@dataclass(frozen=True)
class Step:
    action: Literal['add', 'remove', 'remove_by_id', 'clear', 'apply']
    patterns: list[RulerPattern] = field(default_factory=list[RulerPattern])
    # The argument of `remove` (a label) or `remove_by_id` (an ID).
    argument: str | None = None
    # Apply to the case's document, or to the result of the previous application.
    input: Literal['initial', 'previous'] | None = None


@dataclass(frozen=True)
class ListedPattern:
    """One entry of `SpanRuler.patterns`; `id` None when the entry has no `id` key."""
    label: str
    id: str | None
    tokens: Pattern | None
    phrase: str | None


@dataclass(frozen=True)
class Outcome:
    error: str | None
    length: int
    labels: list[str]
    ids: list[str]
    # The case's probe labels for which `label in ruler` is true.
    contained: list[str]
    patterns: list[ListedPattern]
    matches: list[SpanRecord] | None
    result: Document | None


@dataclass(frozen=True)
class Case:
    id: str
    options: Options
    document: Document
    probes: list[str]
    steps: list[Step]
    outcomes: list[Outcome]


@dataclass(frozen=True)
class Fixture:
    format_version: int
    versions: dict[str, str]
    sources: dict[str, str]
    cases: list[Case]


def document(doc: Doc) -> Document:
    base = entity_document(doc)
    groups = [Group(key, value.name, attrs(value.attrs), [SpanRecord(s.start, s.end, s.label_, s.id_ or None) for s in value])
              for key, value in doc.spans.items()]
    return Document(base.text, base.tokens, base.entities, base.sentences, base.noun_chunks, groups)


def token_pattern(label: str, items: list[Item], id: str | None = None) -> RulerPattern:
    return RulerPattern(label, id, tokens=Pattern(items))


class Runner:
    """Apply steps to a SpanRuler added at the end of a pipeline, as `nlp.add_pipe` does."""

    def __init__(self, nlp: Language, name: str, options: Options) -> None:
        self.nlp = nlp
        self.name = name
        config: dict[str, JsonValue] = {
            'spans_key': options.spans_key,
            'spans_filter': None if options.spans_filter is None else {'@misc': FILTERS[options.spans_filter]},
            'annotate_ents': options.annotate_entities,
            'ents_filter': {'@misc': FILTERS[options.entities_filter]},
            'phrase_matcher_attr': None if options.phrase_attribute == 'ORTH' else options.phrase_attribute,
            'overwrite': options.overwrite,
        }
        ruler = nlp.add_pipe('span_ruler', name, config=config)
        if not isinstance(ruler, SpanRuler):
            raise TypeError('span_ruler did not create a SpanRuler')
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
            raise TypeError('A pattern needs tokens or a phrase')
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

    def run(self, make: Callable[[], Doc], probes: list[str], steps: list[Step]) -> list[Outcome]:
        outcomes: list[Outcome] = []
        previous: Doc | None = None
        try:
            for step in steps:
                error: str | None = None
                found: list[SpanRecord] | None = None
                result: Document | None = None
                if step.action == 'add':
                    self.ruler.add_patterns([self.official(p) for p in step.patterns])
                elif step.action in ('remove', 'remove_by_id'):
                    if step.argument is None:
                        raise TypeError(f'{step.action} needs an argument')
                    try:
                        if step.action == 'remove':
                            self.ruler.remove(step.argument)
                        else:
                            self.ruler.remove_by_id(step.argument)
                    except ValueError as exc:
                        error = type(exc).__name__
                elif step.action == 'clear':
                    self.ruler.clear()
                else:
                    if step.input == 'previous':
                        if previous is None:
                            raise TypeError('no previous application')
                        doc = previous
                    else:
                        doc = make()
                    with warnings.catch_warnings():
                        warnings.filterwarnings('ignore', message=r'\[W036\]')
                        found = [SpanRecord(s.start, s.end, s.label_, s.id_ or None) for s in self.ruler.match(doc)]
                        previous = self.ruler(doc)
                    result = document(previous)
                outcomes.append(Outcome(error, len(self.ruler), list(self.ruler.labels), list(self.ruler.ids),
                                        [p for p in probes if p in self.ruler], self.listed(), found, result))
        finally:
            self.nlp.remove_pipe(self.name)
        return outcomes


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
        labels = {p.label for s in steps for p in s.patterns} | {s.argument for s in steps if s.action == 'remove' and s.argument}
        probes = sorted(labels | {'MISSING'})
        # Each application starts from a serialized copy; `Doc.copy` rejects some empty spans in span
        # groups (E1052).
        saved = doc.to_bytes()
        def make() -> Doc:
            return make_doc(doc.vocab, words=[]).from_bytes(saved)
        return Case(self.case_id, self.options, document(doc), probes, steps, self.runner.run(make, probes, steps))


def add(*patterns: RulerPattern) -> Step:
    return Step('add', list(patterns))


def apply(input: Literal['initial', 'previous'] = 'initial') -> Step:
    return Step('apply', input=input)


def remove(label: str) -> Step:
    return Step('remove', argument=label)


def remove_by_id(id: str) -> Step:
    return Step('remove_by_id', argument=id)


CLEAR = Step('clear')
type SpanTuple = tuple[int, int, str, str | None]


def with_groups(doc: Doc, *groups: tuple[str, str, dict[str, JsonValue], list[SpanTuple]]) -> Doc:
    for key, name, values, spans in groups:
        doc.spans[key] = make_group(doc, name=name, attrs=values,
                                    spans=[make_span(doc, s, e, label=label, span_id=id or '') for s, e, label, id in spans])
    return doc


def with_entities(doc: Doc, entities: list[SpanTuple]) -> Doc:
    # The `Doc.ents` setter, which makes every other token outside.
    doc.set_ents([make_span(doc, s, e, label=label, span_id=id or '') for s, e, label, id in entities], default='outside')
    return doc


def upstream_patterns(case: BuildSteps) -> list[RulerPattern]:
    """The `patterns` fixture of spaCy's test_span_ruler.py."""
    return [
        case.phrase('HELLO', 'hello world', 'hello1'),
        token_pattern('BYE', [lower('bye'), lower('bye')]),
        token_pattern('HELLO', [word('HELLO')], 'hello2'),
        token_pattern('COMPLEX', [word('foo', Repetition('zero_or_more'))]),
        case.phrase('TECH_ORG', 'Apple'),
        case.phrase('TECH_ORG', 'Microsoft'),
    ]


def person_org_date(case: BuildSteps) -> list[RulerPattern]:
    """The `person_org_date_patterns` fixture of spaCy's test_span_ruler.py."""
    return [case.phrase('PERSON', 'Dina'), case.phrase('ORG', 'ACME'), case.phrase('ORG', 'ACM'),
            case.phrase('DATE', 'June 14th')]


def authored(nlp: Language) -> list[Case]:
    cases: list[Case] = []
    def doc(text: str) -> Doc:
        return blank_doc(nlp, text.split())
    case = BuildSteps(nlp, 'init')
    cases.append(case.capture(doc('hello world bye bye'), [add(*upstream_patterns(case)), apply()]))
    for name, overwrite in [('existing', False), ('existing-overwrite', True)]:
        case = BuildSteps(nlp, name, Options(overwrite=overwrite))
        cases.append(case.capture(with_groups(doc('OH HELLO WORLD bye bye'), ('ruler', 'ruler', {}, [(0, 2, '', None)])),
                                  [add(*upstream_patterns(case)), apply()]))
    case = BuildSteps(nlp, 'clear')
    cases.append(case.capture(doc('hello world'), [add(*upstream_patterns(case)), apply(), CLEAR, apply()]))
    case = BuildSteps(nlp, 'remove-labels')
    cases.append(case.capture(doc('Dina founded the company ACME on June 14th'), [
        add(*person_org_date(case)), apply(), remove('PERSON'), apply(), remove('ORG'), remove('DATE'), apply(),
        remove('PERSON'), remove('NE'), remove_by_id('NE')]))
    case = BuildSteps(nlp, 'remove-and-add')
    text = 'I saw him last time we met , this time he brought some flowers , another time some chocolate .'
    cases.append(case.capture(doc(text), [
        add(case.phrase('DATE1', 'last time')), apply(), add(case.phrase('DATE2', 'this time')), apply(), remove('DATE1'),
        apply(), add(case.phrase('DATE1', 'last time')), apply(), add(case.phrase('DATE3', 'another time')), apply(),
        remove('DATE3'), apply()]))
    for name, options in [('spans-filter', Options(spans_filter='first_longest')),
                          ('ents-default-filter', Options(annotate_entities=True))]:
        case = BuildSteps(nlp, name, options)
        cases.append(case.capture(doc('foo bar baz'), [add(case.phrase('FOOBAR', 'foo bar'), case.phrase('BARBAZ', 'bar baz')),
                                                       apply()]))
    # Existing entities with each filter, kept (overwrite off) or discarded (overwrite on).
    for filter in FILTER_KINDS:
        for overwrite in [False, True]:
            case = BuildSteps(nlp, f'ents-{filter}-{"overwrite" if overwrite else "kept"}', Options(
                spans_key=None, annotate_entities=True, entities_filter=filter, overwrite=overwrite))
            cases.append(case.capture(with_entities(doc('foo bar baz a b c d'), [(1, 3, 'BARBAZ', None), (3, 6, 'ABC', 'abc')]), [
                add(case.phrase('FOOBAR', 'foo bar'), case.phrase('BARBAZ', 'bar baz'), case.phrase('D', 'd', 'd'),
                    case.phrase('CD', 'c d')), apply()]))
    # Filters applied to an existing group without overwriting: spaCy extends the group with the
    # filter's result, which already contains the existing spans, so they appear twice.
    for filter in FILTER_KINDS:
        case = BuildSteps(nlp, f'spans-{filter}-existing', Options(spans_filter=filter, overwrite=False))
        existing = with_groups(doc('a b c d e f g'), ('ruler', 'kept-name', {'source': 'manual', 'n': [1, 2]}, [
            (0, 2, 'X', 'x'), (1, 3, 'Y', None), (4, 4, 'EMPTY', None), (5, 5, '', None), (6, 6, 'G', None), (7, 7, 'END', None)]))
        cases.append(case.capture(existing, [add(case.phrase('BC', 'b c'), case.phrase('CDE', 'c d e'), case.phrase('F', 'f'),
                                                 case.phrase('A', 'a')), apply(), apply('previous')]))
    # Spans keys: a custom key, a group stored under the key with another name and attrs, and the
    # position of a replaced key among the others.
    for name, overwrite in [('custom-key-kept', False), ('custom-key-overwrite', True)]:
        case = BuildSteps(nlp, name, Options(spans_key='sc', overwrite=overwrite))
        existing = with_groups(doc('x y z'), ('first', 'first', {}, []), ('sc', 'other', {'k': {'z': 1, 'a': None}}, [(0, 1, 'X', None)]),
                               ('last', 'last', {}, [(2, 3, 'Z', None)]))
        cases.append(case.capture(existing, [add(case.phrase('Y', 'y')), apply()]))
    case = BuildSteps(nlp, 'no-spans-key', Options(spans_key=None))
    cases.append(case.capture(with_groups(doc('x y'), ('ruler', 'ruler', {}, [(0, 1, 'X', None)])), [add(case.phrase('Y', 'y')), apply()]))
    case = BuildSteps(nlp, 'empty-spans-key', Options(spans_key='', annotate_entities=True))
    cases.append(case.capture(doc('x y'), [add(case.phrase('Y', 'y')), apply()]))
    # Identical spans with different labels or IDs are ordered by the string-store hashes of the
    # label, then the ID; symbol labels such as ORG have small hashes.
    same_span: list[tuple[str, Filter | None]] = [('same-span-order', None), ('same-span-first-longest', 'first_longest')]
    for name, filter in same_span:
        case = BuildSteps(nlp, name, Options(spans_filter=filter, annotate_entities=True))
        cases.append(case.capture(doc('a b c d'), [add(
            case.phrase('B', 'b c'), case.phrase('ORG', 'b c'), case.phrase('ZZ', 'b'), case.phrase('B', 'b c', 'y'),
            case.phrase('B', 'b c', 'x'), token_pattern('PERSON', [word('b'), word('c')], 'ORG'),
            token_pattern('PERSON', [word('b'), word('c')])), apply()]))
    # A pattern without an ID and one with the empty ID share a rule; `ids` lists "" only for the
    # second, and `remove_by_id("")` removes both from matching but lists only the second as removed.
    case = BuildSteps(nlp, 'empty-ids')
    cases.append(case.capture(doc('a b c d'), [add(
        case.phrase('X', 'a'), case.phrase('Y', 'b', ''), token_pattern('Y', [word('c')]), case.phrase('Z', 'd', 'z')), apply(),
        remove_by_id(''), apply(), remove_by_id(''), add(case.phrase('X', 'b')), apply(), remove('X'), apply()]))
    case = BuildSteps(nlp, 'remove-by-id')
    cases.append(case.capture(doc('Dina founded DinaCorp and ACME .'), [add(
        case.phrase('PERSON', 'Dina', 'dina'), case.phrase('ORG', 'DinaCorp', 'dina'), token_pattern('ORG', [word('ACME')], 'acme')),
        apply(), remove_by_id('dina'), apply(), remove_by_id('dina'), remove('PERSON'), add(case.phrase('PERSON', 'Dina', 'dina')),
        apply(), remove_by_id('acme'), apply()]))
    # Labels stay known after their patterns are removed or cleared, so removing them again succeeds.
    case = BuildSteps(nlp, 'labels-stay-known')
    cases.append(case.capture(doc('x y'), [add(case.phrase('X', 'x'), token_pattern('Y', [word('y')], 'y')), CLEAR, remove('X'),
                                            remove('Y'), remove_by_id('y'), apply(), add(case.phrase('X', 'y')), apply()]))
    case = BuildSteps(nlp, 'empty-matches-and-patterns', Options(annotate_entities=True))
    cases.append(case.capture(doc('a b a'), [add(
        token_pattern('OPT', [word('z', Repetition('optional'))]), token_pattern('STAR', [word('z', Repetition('zero_or_more'))]),
        case.phrase('EMPTY', ''), token_pattern('A', [word('a'), word('z', Repetition('zero_or_more'))]),
        token_pattern('AB', [word('a'), word('b', Repetition('optional'))])), apply()]))
    case = BuildSteps(nlp, 'duplicate-patterns')
    cases.append(case.capture(doc('a b c'), [add(
        case.phrase('X', 'a b'), case.phrase('X', 'a b'), token_pattern('X', [word('a'), word('b')]),
        token_pattern('X', [word('a'), word('b')]), case.phrase('Y', 'c', 'i'), token_pattern('Y', [word('c')], 'i')), apply()]))
    # An empty label is kept in span groups; setting entities drops it, but it still claims tokens.
    case = BuildSteps(nlp, 'empty-label', Options(annotate_entities=True, overwrite=False))
    cases.append(case.capture(with_entities(doc('w x y z v'), [(0, 1, 'E', 'e')]), [
        add(case.phrase('', 'x y'), token_pattern('', [word('z')], 'q'), case.phrase('W', 'w x'), case.phrase('Y', 'y z'),
            case.phrase('', 'v')), apply(),
        remove(''), apply()]))
    case = BuildSteps(nlp, 'no-patterns', Options(annotate_entities=True))
    cases.append(case.capture(blank_doc(nlp, list('abcd'), [(1, 2, 'X', 'i')], blocked=[(2, 3)]), [apply()]))
    case = BuildSteps(nlp, 'no-patterns-kept', Options(annotate_entities=True, overwrite=False))
    cases.append(case.capture(with_groups(blank_doc(nlp, list('abcd'), [(1, 2, 'X', 'i')], blocked=[(2, 3)]),
                                          ('ruler', 'ruler', {'a': 1}, [(0, 1, 'A', None)])), [apply()]))
    case = BuildSteps(nlp, 'blocked-and-missing', Options(annotate_entities=True, overwrite=False))
    cases.append(case.capture(blank_doc(nlp, list('abcdef'), [], blocked=[(0, 2)], outside=[(5, 6)]),
                              [add(case.phrase('Y', 'a b'), case.phrase('W', 'e')), apply()]))
    unannotated: list[tuple[str, Filter]] = [('no-entity-annotation', 'prioritize_existing'),
                                             ('no-entity-annotation-new', 'prioritize_new')]
    for name, filter in unannotated:
        case = BuildSteps(nlp, name, Options(annotate_entities=True, entities_filter=filter, overwrite=False))
        cases.append(case.capture(doc('a b c'), [apply(), add(case.phrase('Y', 'b')), apply()]))
    for name, overwrite in [('apply-again', False), ('apply-again-overwrite', True)]:
        case = BuildSteps(nlp, name, Options(annotate_entities=True, overwrite=overwrite))
        cases.append(case.capture(blank_doc(nlp, list('abcd'), [(0, 1, 'X', None)]), [
            add(case.phrase('Y', 'b c', 'y')), apply(), apply('previous')]))
    case = BuildSteps(nlp, 'lower-phrases', Options(phrase_attribute='LOWER', annotate_entities=True))
    cases.append(case.capture(nlp.make_doc('Democratic front-runner Joe Biden. Sen. Bernie Sanders joined in.'), [add(
        case.phrase('PERSON', 'joe biden', 'joe-biden'), case.phrase('PERSON', 'bernie sanders')), apply()]))
    case = BuildSteps(nlp, 'empty-document', Options(annotate_entities=True))
    cases.append(case.capture(make_doc(nlp.vocab, words=[]), [add(case.phrase('Y', 'a')), apply()]))
    case = BuildSteps(nlp, 'unicode-and-spacing', Options(annotate_entities=True))
    cases.append(case.capture(nlp.make_doc('Café  naïve 東京 ok'), [add(case.phrase('X', 'naïve 東京'), token_pattern('Y', [word('Café')])),
                                                                    apply()]))
    return cases


def model_cases(nlp: Language) -> list[Case]:
    text = 'Apple is looking at buying a U.K. startup for $1 billion. Tim Cook visited London.'
    cases: list[Case] = []
    # Documents are processed before a case adds its ruler to the pipeline.
    predicted = nlp(text)
    after_ner: list[Filter] = ['prioritize_existing', 'prioritize_new', 'first_longest']
    for filter in after_ner:
        case = BuildSteps(nlp, f'after-ner-{filter}', Options(annotate_entities=True, entities_filter=filter, overwrite=False))
        cases.append(case.capture(predicted, [add(
            case.phrase('PERSON', 'Tim Cook', 'tim-cook'), token_pattern('ORG', [lower('apple')], 'apple'),
            case.phrase('ORG', 'U.K. startup'), case.phrase('MONEY', '$1')), apply()]))
    lemmas = nlp('She buys apples and he bought pears.')
    case = BuildSteps(nlp, 'lemma-phrases', Options(phrase_attribute='LEMMA'))
    cases.append(case.capture(lemmas, [add(case.phrase('ACT', 'buy'), case.phrase('FRUIT', 'apple')), apply()]))
    with nlp.select_pipes(disable=['ner']):
        doc = nlp(text)
    case = BuildSteps(nlp, 'without-ner', Options(annotate_entities=True, overwrite=False))
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
    entities: list[SpanTuple] = []
    for _ in range(rng.randrange(0, 3)):
        span = take()
        if span is not None:
            entities.append((*span, rng.choice(['X', 'ORG']), rng.choice(['e', None])))
    blocked = [s for s in [take() if rng.random() < 0.3 else None] if s is not None]
    annotated = rng.random() < 0.8
    doc = blank_doc(nlp, words, entities if annotated else None, blocked=blocked if annotated else None)
    key = rng.choice(['ruler', 'ruler', 'sc', None])
    if rng.random() < 0.6:
        spans: list[SpanTuple] = []
        for _ in range(rng.randrange(0, 4)):
            start = rng.randrange(9)
            spans.append((start, min(8, start + rng.randrange(0, 3)), rng.choice(['G', 'P', '']), rng.choice(['g', None])))
        with_groups(doc, (rng.choice(['ruler', 'sc']), 'existing', {'n': index}, spans))
    spans_filters: list[Filter | None] = [None, None, *FILTER_KINDS]
    options = Options(spans_key=key, spans_filter=rng.choice(spans_filters), annotate_entities=rng.random() < 0.6,
                      entities_filter=rng.choice(FILTER_KINDS), overwrite=rng.random() < 0.5)
    case = BuildSteps(nlp, f'random-{index}', options)
    def patterns() -> list[RulerPattern]:
        result: list[RulerPattern] = []
        for _ in range(rng.randrange(1, 6)):
            label, id = rng.choice(['P', 'Q', 'ORG', 'PERSON']), rng.choice([None, '', 'p', 'q'])
            if rng.random() < 0.5:
                result.append(case.phrase(label, ' '.join(rng.choice('abcd') for _ in range(rng.randrange(1, 4))), id))
            else:
                items = [word(rng.choice('abcd'), Repetition(rng.choice(['once', 'once', 'optional', 'zero_or_more', 'one_or_more'])))
                         for _ in range(rng.randrange(1, 4))]
                result.append(token_pattern(label, items, id))
        return result
    steps = [add(*patterns()), apply()]
    for _ in range(rng.randrange(0, 3)):
        choice = rng.random()
        if choice < 0.3:
            steps.append(remove(rng.choice(['P', 'Q', 'ORG', 'PERSON'])))
        elif choice < 0.6:
            steps.append(remove_by_id(rng.choice(['', 'p', 'q'])))
        else:
            steps.append(add(*patterns()))
        steps.append(apply(rng.choice(['initial', 'previous'])))
    return case.capture(doc, steps)


def build() -> Fixture:
    versions, sources = verify_source(SOURCES)
    warnings.filterwarnings('ignore', message=r'\[W036\]')
    model = spacy.load('en_core_web_md')
    if model.meta.get('version') != '3.8.0':
        raise ValueError('Expected en_core_web_md 3.8.0')
    versions['en_core_web_md'] = '3.8.0'
    cases = authored(spacy.blank('en')) + model_cases(model)
    rng = random.Random(2022)
    nlp = spacy.blank('en')
    while len(cases) < 100:
        cases.append(random_case(nlp, rng, len(cases)))
    return Fixture(1, versions, sources, cases)


class Arguments(argparse.Namespace):
    output: Path = Path('target/reports/span-ruler.json')


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
