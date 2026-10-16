"""Freeze spaCy Doc.set_ents results for native entity annotation updates."""
import argparse
from dataclasses import asdict, dataclass
import hashlib
import importlib.metadata
import json
from pathlib import Path
import random
from typing import Literal

import spacy
from spacy.tokens import Doc as make_doc
from spacy.tokens import Span as make_span

from json_types import json_object, json_string, read_json
from reference_types import Doc

ROOT = Path(__file__).resolve().parent.parent
# set_ents and the Doc.ents getter, and the Span bounds check (E035).
SOURCES = ['spacy/tokens/doc.pyx', 'spacy/tokens/span.pyx']
type Default = Literal['outside', 'missing', 'blocked', 'unmodified']
DEFAULTS: list[Default] = ['outside', 'missing', 'blocked', 'unmodified']


@dataclass(frozen=True)
class EntitySpan:
    start: int
    end: int
    label: str


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
    """spaCy's ent_iob_ and ent_type_, with None for the missing state (IOB 0)."""
    iob: str | None
    type: str | None


@dataclass(frozen=True)
class State:
    error: str | None
    tokens: list[TokenEntity]
    # None when spaCy's has_annotation("ENT_IOB") is false; an empty document is annotated.
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


def verify_source(sources: list[str] = SOURCES) -> tuple[dict[str, str], dict[str, str]]:
    lock = json_object(json_object(read_json(ROOT / 'reference/source-lock.json'))['spacy'])
    if spacy.__version__ != json_string(lock['version']) or spacy.__version__ != '3.8.14':
        raise ValueError('Expected spaCy 3.8.14')
    files = json_object(lock['files'])
    hashes: dict[str, str] = {}
    for source in sources:
        actual = hashlib.sha256(Path(str(importlib.metadata.distribution('spacy').locate_file(source))).read_bytes()).hexdigest()
        if actual != json_string(files[source]):
            raise ValueError(f'Source mismatch: {source}')
        hashes[source] = actual
    return {'spacy': spacy.__version__}, hashes


def state(doc: Doc, error: str | None = None) -> State:
    # A missing tag is recorded without a type; check that spaCy stores none there.
    if any(t.ent_type_ for t in doc if not t.ent_iob_):
        raise ValueError('spaCy kept an entity type on a token with missing entity annotation')
    tokens = [TokenEntity(t.ent_iob_ or None, t.ent_type_ if t.ent_iob_ else None) for t in doc]
    entities = [EntitySpan(e.start, e.end, e.label_) for e in doc.ents] if doc.has_annotation('ENT_IOB') else None
    return State(error, tokens, entities)


def apply(doc: Doc, update: Update) -> State:
    before = state(doc)
    try:
        spans = [make_span(doc, e.start, e.end, label=e.label) for e in update.entities]
        blocked, missing, outside = ([make_span(doc, r.start, r.end) for r in ranges]
                                     for ranges in (update.blocked, update.missing, update.outside))
        doc.set_ents(spans, blocked=blocked, missing=missing, outside=outside, default=update.default)
    except (IndexError, ValueError) as exc:
        after = state(doc)
        if after != before:
            raise ValueError('spaCy changed the document before rejecting an update') from exc
        return state(doc, type(exc).__name__)
    return state(doc)


def update(entities: list[tuple[int, int, str]] | None = None, *, blocked: list[tuple[int, int]] | None = None,
           missing: list[tuple[int, int]] | None = None, outside: list[tuple[int, int]] | None = None,
           default: Default = 'outside') -> Update:
    def ranges(values: list[tuple[int, int]] | None) -> list[TokenRange]:
        return [TokenRange(start, end) for start, end in values or []]
    return Update([EntitySpan(s, e, label) for s, e, label in entities or []], ranges(blocked), ranges(missing), ranges(outside), default)


def capture(name: str, doc: Doc, updates: list[Update]) -> Case:
    words = [t.text for t in doc]
    spaces = [t.whitespace_ == ' ' for t in doc]
    initial = state(doc)
    return Case(name, words, spaces, initial, updates, [apply(doc, u) for u in updates])


def random_updates(rng: random.Random, length: int, count: int) -> list[Update]:
    labels = ['PERSON', 'ORG', 'GPE', '']
    result: list[Update] = []
    for _ in range(count):
        def take() -> tuple[int, int]:
            start = rng.randrange(length)
            return start, min(length, start + rng.randrange(0, 4))
        entities: list[tuple[int, int, str]] = []
        others: dict[str, list[tuple[int, int]]] = {'blocked': [], 'missing': [], 'outside': []}
        used: set[int] = set()
        def fits(span: tuple[int, int]) -> bool:
            return not used.intersection(range(*span))
        for _ in range(rng.randrange(0, 4)):
            span = take()
            if fits(span) or rng.random() < 0.05:
                used.update(range(*span))
                entities.append((*span, rng.choice(labels)))
        for kind in others:
            if rng.random() < 0.4:
                span = take()
                if fits(span) or rng.random() < 0.05:
                    used.update(range(*span))
                    others[kind].append(span)
        if rng.random() < 0.04:
            entities.append((length - 1, length + 1, 'ORG'))
        result.append(update(entities, blocked=others['blocked'], missing=others['missing'],
                             outside=others['outside'], default=rng.choice(DEFAULTS)))
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
        capture('defaults', blank(), [
            update([(1, 3, 'X')]), update([(1, 3, 'X')], default='missing'),
            update([(1, 3, 'X')], default='blocked'), update([(4, 5, 'Y')], default='unmodified'),
        ]),
        capture('unmodified-on-unannotated', blank(), [update([(1, 3, 'X')], default='unmodified')]),
        capture('empty-and-unlabeled', blank(), [
            update([(2, 2, 'X')]), update([(0, 2, 'X'), (1, 3, '')]),
            update(blocked=[(1, 3)]), update([], default='missing'), update([(3, 3, 'X')], default='unmodified'),
        ]),
        capture('repair-inside-tags', blank(), [
            update([(0, 4, 'X')]), update(outside=[(0, 1)], default='unmodified'),
            update([(0, 4, 'X')]), update([(2, 3, 'Y')], default='unmodified'),
            update([(0, 4, 'X')]), update(missing=[(1, 2)], default='unmodified'),
            update([(0, 4, 'X')]), update(blocked=[(1, 2)], default='unmodified'),
        ]),
        capture('rejected-updates', blank(), [
            update([(0, 2, 'X')]), update([(3, 2, 'X')]), update([(4, 9, 'X')]),
            update([(0, 2, 'X')], blocked=[(1, 3)]), update([(0, 2, 'X'), (0, 2, 'X')]),
            update(missing=[(0, 3)], outside=[(2, 4)], default='unmodified'),
            update(blocked=[(0, 7)]), update([(1, 2, 'Y')], default='unmodified'),
        ]),
        # An unlabeled entity is ignored, but spaCy's Span constructor still checks its bounds.
        capture('unlabeled-out-of-bounds', blank(), [update([(0, 2, 'X')]), update([(4, 8, '')]), update([(5, 5, '')])]),
        # An inside tag of the same type is not repaired, so a shorter entity merges with what follows.
        capture('same-label-continuation', blank(), [
            update([(0, 3, 'X')]), update([(0, 1, 'X')], default='unmodified'),
            update([(0, 3, 'X')]), update([(1, 2, 'X')], default='unmodified'),
        ]),
        # spaCy's test_doc_set_ents "add ents, invalid IOB repaired" sequence.
        capture('upstream-repair-sequence', blank(5), [
            update([(0, 1, 'L10'), (1, 3, 'L11')]), update([(0, 2, 'L12')], default='unmodified'),
        ]),
        capture('empty-document', make_doc(vocab, words=[]), [
            update(), update([(0, 0, 'X')], default='missing'), update([(0, 1, 'X')]),
        ]),
    ]
    # Model predictions as the starting state, then rule-style edits that keep them.
    text = 'Apple is looking at buying a U.K. startup for $1 billion. Tim Cook visited London.'
    predicted = nlp(text)
    cases.append(capture('after-ner', predicted, [
        update([(0, 1, 'TECH')], default='unmodified'),
        update(outside=[(6, 8)], default='unmodified'),
        update([(13, 15, 'CEO')], blocked=[(16, 17)], default='unmodified'),
        update([(2, 4, 'ACTION')], default='missing'),
    ]))
    rng = random.Random(1010)
    for index in range(24):
        cases.append(capture(f'random-{index}', blank(9), random_updates(rng, 9, 12)))
    return Fixture(1, versions, sources, cases)


class Options(argparse.Namespace):
    output: Path = Path('target/reports/entity-updates.json')


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
