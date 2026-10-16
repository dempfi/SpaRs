"""Freeze spaCy span groups: Doc.spans keys, group names, attrs and spans after each edit."""
import argparse
from dataclasses import asdict, dataclass
import json
from pathlib import Path
import random
from typing import Literal

import spacy
from spacy.tokens import Doc as make_doc
from spacy.tokens import Span as make_span
from spacy.tokens import SpanGroup as make_group

from entity_update_reference import verify_source
from json_types import JsonValue, validate_json
from reference_types import Doc, Span, SpanGroup, Vocab

SOURCES = ['spacy/tokens/doc.pyx', 'spacy/tokens/span.pyx', 'spacy/tokens/span_group.pyx',
           'spacy/tokens/_dict_proxies.py']

type Action = Literal['set', 'set_group', 'delete', 'append', 'extend', 'extend_group', 'concat', 'set_item', 'copy']


@dataclass(frozen=True)
class SpanRecord:
    start: int
    end: int
    label: str
    # spaCy's Span.id_, with None for the empty ID.
    id: str | None


@dataclass(frozen=True)
class Step:
    """One edit of Doc.spans. Fields not used by the action are None.

    - set: doc.spans[key] = spans
    - set_group: doc.spans[key] = SpanGroup(doc, name=name, attrs=attrs, spans=spans)
    - delete: del doc.spans[key]
    - append: doc.spans[key].append(spans[0])
    - extend: doc.spans[key].extend(spans)
    - extend_group: doc.spans[key].extend(doc.spans[other])
    - concat: doc.spans[key] = doc.spans[other] + doc.spans[second]
    - set_item: doc.spans[key][index] = spans[0]
    - copy: replace the document with doc.copy()
    """
    action: Action
    key: str | None
    spans: list[SpanRecord] | None
    name: str | None
    attrs: dict[str, JsonValue] | None
    other: str | None
    second: str | None
    index: int | None


@dataclass(frozen=True)
class Group:
    key: str
    name: str
    attrs: dict[str, JsonValue]
    spans: list[SpanRecord]
    has_overlap: bool


@dataclass(frozen=True)
class State:
    # The exception type spaCy raised for the step, with the groups unchanged.
    error: str | None
    groups: list[Group]


@dataclass(frozen=True)
class Case:
    id: str
    words: list[str]
    spaces: list[bool]
    steps: list[Step]
    states: list[State]


@dataclass(frozen=True)
class Fixture:
    format_version: int
    versions: dict[str, str]
    sources: dict[str, str]
    cases: list[Case]


def attrs(value: object) -> dict[str, JsonValue]:
    checked = validate_json(value)
    if not isinstance(checked, dict):
        raise TypeError('span group attrs must be a JSON object')
    return checked


def record(span: Span) -> SpanRecord:
    return SpanRecord(span.start, span.end, span.label_, span.id_ or None)


def group(key: str, value: SpanGroup) -> Group:
    return Group(key, value.name, attrs(value.attrs), [record(s) for s in value], value.has_overlap)


def state(doc: Doc, error: str | None = None) -> State:
    return State(error, [group(key, value) for key, value in doc.spans.items()])


def span(doc: Doc, value: SpanRecord) -> Span:
    return make_span(doc, value.start, value.end, label=value.label, span_id=value.id or '')


def run(doc: Doc, step: Step) -> Doc:
    # Malformed steps raise TypeError, which `capture` does not record as a spaCy rejection.
    def key(value: str | None = step.key) -> str:
        if value is None:
            raise TypeError(f'{step.action} step needs a key')
        return value
    def spans() -> list[Span]:
        return [span(doc, s) for s in step.spans or []]
    def single() -> Span:
        if len(step.spans or []) != 1:
            raise TypeError(f'{step.action} step needs exactly one span')
        return spans()[0]
    match step.action:
        case 'set':
            doc.spans[key()] = spans()
        case 'set_group':
            doc.spans[key()] = make_group(doc, name=step.name or '', attrs=step.attrs or {}, spans=spans())
        case 'delete':
            del doc.spans[key()]
        case 'append':
            doc.spans[key()].append(single())
        case 'extend':
            doc.spans[key()].extend(spans())
        case 'extend_group':
            doc.spans[key()].extend(doc.spans[key(step.other)])
        case 'concat':
            doc.spans[key()] = doc.spans[key(step.other)] + doc.spans[key(step.second)]
        case 'set_item':
            if step.index is None:
                raise TypeError('set_item step needs an index')
            # Python evaluates the new span, and so its bounds, before looking up the key and index.
            doc.spans[key()][step.index] = single()
        case 'copy':
            return doc.copy()
    return doc


def check_storage(doc: Doc, expected: State) -> None:
    """Saving and copying keep every group; SpaRs snapshots must do the same. Doc.copy rebuilds spans from
    character offsets and raises E1052 for most empty spans, so copying is checked only without them."""
    # spaCy saves groups with identical content as one entry listing their keys, so loading moves a later key
    # next to the first; only the key order changes.
    restored = make_doc(doc.vocab, words=[]).from_bytes(doc.to_bytes())
    def by_key(value: State) -> list[Group]:
        return sorted(value.groups, key=lambda g: g.key)
    if by_key(state(restored)) != by_key(expected):
        raise ValueError('spaCy changed span groups when saving')
    if all(s.start < s.end for g in expected.groups for s in g.spans) and state(doc.copy()) != expected:
        raise ValueError('spaCy changed span groups when copying')


def capture(name: str, doc: Doc, steps: list[Step]) -> Case:
    words = [t.text for t in doc]
    spaces = [t.whitespace_ == ' ' for t in doc]
    states: list[State] = []
    for step in steps:
        before = state(doc)
        try:
            doc = run(doc, step)
        except (IndexError, KeyError, ValueError) as exc:
            if state(doc) != before:
                raise ValueError('spaCy changed span groups before rejecting an edit') from exc
            states.append(state(doc, type(exc).__name__))
            continue
        current = state(doc)
        check_storage(doc, current)
        states.append(current)
    return Case(name, words, spaces, steps, states)


type SpanTuple = tuple[int, int] | tuple[int, int, str] | tuple[int, int, str, str]


def spans(*values: SpanTuple) -> list[SpanRecord]:
    result: list[SpanRecord] = []
    for value in values:
        label = value[2] if len(value) > 2 else ''
        id = value[3] if len(value) > 3 else None
        result.append(SpanRecord(value[0], value[1], label, id))
    return result


def step(action: Action, key: str | None = None, values: list[SpanRecord] | None = None, *, name: str | None = None,
         attrs: dict[str, JsonValue] | None = None, other: str | None = None, second: str | None = None,
         index: int | None = None) -> Step:
    return Step(action, key, values, name, attrs, other, second, index)


def authored(blank: 'Blank') -> list[Case]:
    return [
        capture('set-keeps-order-and-overlaps', blank(), [
            step('set', 'b', spans((0, 2, 'X'), (1, 3, 'Y', 'y'), (0, 2, 'X'), (2, 2, 'E'))),
            step('set', 'a', spans((3, 4))),
            step('set', 'b', spans((4, 6, 'Z'))),
            step('set', 'c', []),
        ]),
        # A group's name and key can differ, and attrs are kept as given.
        capture('named-groups-and-attrs', blank(), [
            step('set_group', 'key', spans((0, 1)), name='other',
                 attrs={'annotator': 'matt', 'n': 2, 'f': 0.5, 'ok': True, 'none': None, 'list': [1, 'a', {'x': []}]}),
            step('set_group', 'same', spans((1, 2)), name='same', attrs={}),
            step('set_group', 'empty-name', [], name='', attrs={'k': 'v'}),
        ]),
        capture('delete', blank(), [
            step('set', 'a', spans((0, 1))), step('set', 'b', spans((1, 2))), step('delete', 'a'),
            step('delete', 'a'), step('set', 'a', spans((2, 3))),
        ]),
        capture('append-and-extend', blank(), [
            step('set', 'g', spans((0, 1))), step('append', 'g', spans((0, 6, 'ALL'))),
            step('extend', 'g', spans((2, 3), (2, 3), (5, 6, 'L', 'i'))), step('extend', 'g', []),
            step('append', 'missing', spans((0, 1))),
        ]),
        # Extending with another group adds its spans and only the attrs the group lacks.
        capture('extend-with-group', blank(), [
            step('set_group', 'a', spans((0, 1)), name='a', attrs={'shared': 'a', 'only_a': 1}),
            step('set_group', 'b', spans((1, 3, 'B')), name='b', attrs={'shared': 'b', 'only_b': [2]}),
            # Extending a group with itself is left out: spaCy inserts a vector range into itself.
            step('extend_group', 'a', other='b'),
            step('extend_group', 'a', other='missing'),
        ]),
        # `+` copies the first group, keeping its name, and merges attrs the same way.
        capture('concat', blank(), [
            step('set_group', 'a', spans((0, 1)), name='first', attrs={'k': 1}),
            step('set_group', 'b', spans((1, 2), (0, 1)), name='second', attrs={'k': 2, 'j': 3}),
            step('concat', 'c', other='a', second='b'), step('concat', 'a', other='a', second='a'),
            step('concat', 'd', other='b', second='missing'),
        ]),
        capture('set-item', blank(), [
            step('set', 'g', spans((0, 1), (1, 2), (2, 3))), step('set_item', 'g', spans((4, 6, 'N', 'n')), index=1),
            step('set_item', 'g', spans((5, 6)), index=-1), step('set_item', 'g', spans((5, 6)), index=-3),
            step('set_item', 'g', spans((5, 6)), index=3), step('set_item', 'g', spans((5, 6)), index=-4),
        ]),
        capture('invalid-spans', blank(), [
            step('set', 'g', spans((0, 1))), step('set', 'g', spans((0, 7))), step('set', 'g', spans((3, 2))),
            step('append', 'g', spans((6, 6))), step('append', 'g', spans((6, 7))),
            step('set_item', 'g', spans((0, 9)), index=0),
        ]),
        capture('has-overlap', blank(), [
            step('set', 'touching', spans((0, 1), (1, 2), (2, 4))),
            step('set', 'nested', spans((0, 6), (2, 3))),
            step('set', 'unsorted', spans((4, 6), (0, 5))),
            step('set', 'duplicate', spans((1, 2), (1, 2))),
            step('set', 'empty-spans', spans((2, 2), (2, 2), (1, 3))),
        ]),
        capture('copy', blank(), [
            step('set_group', 'g', spans((0, 2, 'X', 'x')), name='n', attrs={'a': [1]}), step('copy'),
            step('append', 'g', spans((2, 3))),
        ]),
        capture('empty-document', make_doc(blank.vocab, words=[]), [
            step('set', 'g', []), step('set', 'g', spans((0, 0))), step('set', 'h', spans((0, 1))),
        ]),
        capture('unicode-keys-and-labels', blank(), [
            step('set_group', 'grüße', spans((0, 1, 'ÉTIQ', 'ïd')), name='名前', attrs={'ключ': 'значение'}),
        ]),
    ]


class Blank:
    def __init__(self, vocab: Vocab) -> None:
        self.vocab = vocab

    def __call__(self, count: int = 6) -> Doc:
        return make_doc(self.vocab, words=[chr(ord('a') + i) for i in range(count)])


def random_steps(rng: random.Random, length: int, count: int) -> list[Step]:
    keys = ['a', 'b', 'c']
    labels = ['', 'X', 'Y']
    ids: list[str | None] = [None, None, 'i', 'j']
    def random_spans(limit: int) -> list[SpanRecord]:
        result: list[SpanRecord] = []
        for _ in range(rng.randrange(0, limit + 1)):
            # Nonempty, so the copy step stays valid in spaCy.
            start = rng.randrange(length)
            end = min(length, start + rng.randrange(1, 4))
            result.append(SpanRecord(start, end, rng.choice(labels), rng.choice(ids)))
        return result
    def random_attrs() -> dict[str, JsonValue]:
        return {k: rng.randrange(3) for k in rng.sample(['p', 'q', 'r'], rng.randrange(0, 3))}
    # Start with every key set, so most edits find their groups; deletions make later misses possible.
    result: list[Step] = [step('set_group', k, random_spans(3), name=k, attrs=random_attrs()) for k in keys]
    for _ in range(count):
        action: Action = rng.choice(['set', 'set', 'set_group', 'delete', 'append', 'extend', 'extend_group', 'concat',
                                     'set_item', 'copy'])
        key = rng.choice(keys)
        match action:
            case 'set':
                result.append(step('set', key, random_spans(3)))
            case 'set_group':
                result.append(step('set_group', key, random_spans(3), name=rng.choice(keys + ['']), attrs=random_attrs()))
            case 'delete':
                result.append(step('delete', key))
            case 'append':
                result.append(step('append', key, random_spans(1) or spans((0, 1))))
            case 'extend':
                result.append(step('extend', key, random_spans(3)))
            case 'extend_group':
                result.append(step('extend_group', key, other=rng.choice([k for k in keys if k != key])))
            case 'concat':
                result.append(step('concat', key, other=rng.choice(keys), second=rng.choice(keys)))
            case 'set_item':
                result.append(step('set_item', key, random_spans(1) or spans((1, 2)), index=rng.randrange(-3, 3)))
            case 'copy':
                result.append(step('copy'))
    return result


def build() -> Fixture:
    versions, sources = verify_source(SOURCES)
    nlp = spacy.load('en_core_web_md')
    if nlp.meta.get('version') != '3.8.0':
        raise ValueError('Expected en_core_web_md 3.8.0')
    versions['en_core_web_md'] = '3.8.0'
    blank = Blank(nlp.vocab)
    cases = authored(blank)
    # Span groups are independent of the pipeline's annotations.
    cases.append(capture('after-pipeline', nlp('Tim Cook visited London.'), [
        step('set', 'ents', spans((0, 2, 'PERSON', 'tim'), (3, 4, 'GPE'))),
        step('set_group', 'sentences', spans((0, 5)), name='sents', attrs={'source': 'parser'}),
    ]))
    rng = random.Random(2022)
    for index in range(24):
        cases.append(capture(f'random-{index}', blank(8), random_steps(rng, 8, 12)))
    return Fixture(1, versions, sources, cases)


class Options(argparse.Namespace):
    output: Path = Path('target/reports/span-groups.json')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    options = Options()
    parser.parse_args(namespace=options)
    if options.output.exists():
        raise ValueError('Refusing to overwrite frozen output')
    fixture = build()
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(asdict(fixture), ensure_ascii=False, indent=2) + '\n')
    states = [s for c in fixture.cases for s in c.states]
    print(f'{len(fixture.cases)} cases; {len(states)} steps; {sum(s.error is not None for s in states)} rejected')


if __name__ == '__main__':
    main()
