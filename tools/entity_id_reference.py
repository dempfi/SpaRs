"""Freeze spaCy entity IDs through Doc.set_ents and the Doc.ents getter."""
import argparse
from dataclasses import asdict, dataclass
import json
from pathlib import Path
import random

import spacy
from spacy.tokens import Doc as make_doc
from spacy.tokens import Span as make_span

from entity_update_reference import DEFAULTS, Default, verify_source
from reference_types import Doc


@dataclass(frozen=True)
class EntitySpan:
    start: int
    end: int
    label: str
    # spaCy's Span.id_, with None for the empty ID.
    id: str | None


@dataclass(frozen=True)
class TokenRange:
    start: int
    end: int


@dataclass(frozen=True)
class Update:
    entities: list[EntitySpan]
    blocked: list[TokenRange]
    missing: list[TokenRange]
    outside: list[TokenRange]
    default: Default


@dataclass(frozen=True)
class TokenEntity:
    """spaCy's ent_iob_, ent_type_ and ent_id_, with None for the missing state and the empty ID."""
    iob: str | None
    type: str | None
    id: str | None


@dataclass(frozen=True)
class State:
    error: str | None
    tokens: list[TokenEntity]
    entities: list[EntitySpan] | None


@dataclass(frozen=True)
class Case:
    id: str
    words: list[str]
    spaces: list[bool]
    initial: State
    updates: list[Update]
    states: list[State]


@dataclass(frozen=True)
class Fixture:
    format_version: int
    versions: dict[str, str]
    sources: dict[str, str]
    cases: list[Case]


def state(doc: Doc, error: str | None = None) -> State:
    if any(t.ent_type_ for t in doc if not t.ent_iob_):
        raise ValueError('spaCy kept an entity type on a token with missing entity annotation')
    tokens = [TokenEntity(t.ent_iob_ or None, t.ent_type_ if t.ent_iob_ else None, t.ent_id_ or None) for t in doc]
    entities = ([EntitySpan(e.start, e.end, e.label_, e.id_ or None) for e in doc.ents]
                if doc.has_annotation('ENT_IOB') else None)
    return State(error, tokens, entities)


def apply(doc: Doc, update: Update) -> State:
    before = state(doc)
    try:
        spans = [make_span(doc, e.start, e.end, label=e.label, span_id=e.id or '') for e in update.entities]
        blocked, missing, outside = ([make_span(doc, r.start, r.end) for r in ranges]
                                     for ranges in (update.blocked, update.missing, update.outside))
        doc.set_ents(spans, blocked=blocked, missing=missing, outside=outside, default=update.default)
    except (IndexError, ValueError) as exc:
        if state(doc) != before:
            raise ValueError('spaCy changed the document before rejecting an update') from exc
        return state(doc, type(exc).__name__)
    return state(doc)


type Entity = tuple[int, int, str, str | None]


def update(entities: list[Entity] | None = None, *, blocked: list[tuple[int, int]] | None = None,
           missing: list[tuple[int, int]] | None = None, outside: list[tuple[int, int]] | None = None,
           default: Default = 'outside') -> Update:
    def ranges(values: list[tuple[int, int]] | None) -> list[TokenRange]:
        return [TokenRange(start, end) for start, end in values or []]
    return Update([EntitySpan(s, e, label, id) for s, e, label, id in entities or []], ranges(blocked),
                  ranges(missing), ranges(outside), default)


def capture(name: str, doc: Doc, updates: list[Update]) -> Case:
    words = [t.text for t in doc]
    spaces = [t.whitespace_ == ' ' for t in doc]
    initial = state(doc)
    return Case(name, words, spaces, initial, updates, [apply(doc, u) for u in updates])


def random_updates(rng: random.Random, length: int, count: int) -> list[Update]:
    labels = ['PERSON', 'ORG', '']
    ids: list[str | None] = ['a', 'b', 'c', None, None]
    result: list[Update] = []
    for _ in range(count):
        used: set[int] = set()
        def take() -> tuple[int, int] | None:
            start = rng.randrange(length)
            span = (start, min(length, start + rng.randrange(1, 4)))
            if used.intersection(range(*span)):
                return None
            used.update(range(*span))
            return span
        entities: list[Entity] = []
        for _ in range(rng.randrange(0, 4)):
            span = take()
            if span is not None:
                entities.append((*span, rng.choice(labels), rng.choice(ids)))
        def maybe() -> list[tuple[int, int]]:
            span = take() if rng.random() < 0.3 else None
            return [] if span is None else [span]
        blocked = maybe()
        missing = maybe()
        outside = maybe()
        result.append(update(entities, blocked=blocked, missing=missing, outside=outside, default=rng.choice(DEFAULTS)))
    return result


def build() -> Fixture:
    versions, sources = verify_source()
    nlp = spacy.load('en_core_web_md')
    if nlp.meta.get('version') != '3.8.0':
        raise ValueError('Expected en_core_web_md 3.8.0')
    versions['en_core_web_md'] = '3.8.0'
    vocab = nlp.vocab
    def blank(count: int = 6) -> Doc:
        return make_doc(vocab, words=[chr(ord('a') + i) for i in range(count)])
    cases = [
        capture('ids-written', blank(), [update([(0, 2, 'X', 'a'), (3, 4, 'Y', None)])]),
        # set_ents writes an ID only when the span has one, so an entity without an ID keeps the
        # token's previous ID, and the Doc.ents getter reports it.
        capture('id-kept-without-span-id', blank(), [
            update([(0, 3, 'X', 'a')]), update([(0, 3, 'X', None)]), update([(1, 2, 'Y', None)]),
            update([(1, 2, 'Y', '')]),
        ]),
        capture('id-replaced', blank(), [update([(0, 3, 'X', 'a')]), update([(0, 3, 'X', 'b')]), update([(1, 2, 'X', 'c')])]),
        # Outside, missing and blocked tokens keep their IDs, whichever way they are set.
        capture('non-entity-tokens-keep-ids', blank(), [
            update([(0, 6, 'X', 'a')]), update(), update([(0, 6, 'X', 'b')]), update(missing=[(0, 2)], default='unmodified'),
            update(blocked=[(2, 4)], default='unmodified'), update(outside=[(4, 6)], default='unmodified'),
            update([], default='missing'), update([], default='blocked'),
        ]),
        # The getter takes an entity's ID from its first token.
        capture('getter-uses-first-token', blank(), [
            update([(0, 3, 'X', 'a')]), update([(1, 2, 'X', 'b')], default='unmodified'),
            update([(0, 4, 'X', 'a')]), update([(2, 3, 'Y', 'b')], default='unmodified'),
        ]),
        capture('repaired-tags-keep-ids', blank(), [
            update([(0, 4, 'X', 'a')]), update(outside=[(0, 1)], default='unmodified'),
        ]),
        # An entity with an empty label is ignored, including its ID.
        capture('unlabeled-id-ignored', blank(), [update([(0, 2, 'X', 'a')]), update([(0, 2, '', 'b')], default='unmodified')]),
        capture('adjacent-same-label', blank(), [update([(0, 2, 'X', 'a'), (2, 4, 'X', 'b'), (4, 6, 'X', None)])]),
        capture('rejected-updates-keep-ids', blank(), [
            update([(0, 2, 'X', 'a')]), update([(1, 3, 'Y', 'b'), (2, 4, 'Y', 'c')]), update([(4, 9, 'Y', 'b')]),
        ]),
        capture('empty-document', make_doc(vocab, words=[]), [update([], default='missing'), update([(0, 0, 'X', 'a')])]),
    ]
    text = 'Apple is looking at buying a U.K. startup for $1 billion. Tim Cook visited London.'
    cases.append(capture('after-ner', nlp(text), [
        update([(0, 1, 'ORG', 'apple')], default='unmodified'),
        update([(13, 15, 'PERSON', 'tim-cook'), (16, 17, 'GPE', 'london')], default='unmodified'),
        update([(13, 15, 'PERSON', None)], default='unmodified'),
    ]))
    rng = random.Random(2020)
    for index in range(16):
        cases.append(capture(f'random-{index}', blank(8), random_updates(rng, 8, 10)))
    return Fixture(1, versions, sources, cases)


class Options(argparse.Namespace):
    output: Path = Path('target/reports/entity-ids.json')


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
    print(f'{len(fixture.cases)} cases; {len(states)} updates; {sum(s.error is not None for s in states)} rejected')


if __name__ == '__main__':
    main()
