"""Frozen SpanRuler fixture checks with an independent recomputation of every annotation."""
from collections import Counter
from dataclasses import asdict, replace
import json
import unittest

import spacy

from entity_ruler_reference import EntityRecord
from entity_update_reference import ROOT
from span_ruler_reference import Document, Filter, Group, Options, SpanRecord, build


def first_longest(first: list[SpanRecord], second: list[SpanRecord], reached: Counter[str]) -> list[SpanRecord]:
    """spaCy's `filter_chain_spans`: longest first, then earliest, checking only the end tokens."""
    chained = first + second
    order = sorted(range(len(chained)), key=lambda i: (chained[i].start - chained[i].end, chained[i].start, i))
    seen: set[int] = set()
    kept: list[SpanRecord] = []
    for i in order:
        span = chained[i]
        if span.start not in seen and span.end - 1 not in seen:
            kept.append(span)
            seen.update(range(span.start, span.end))
            reached['empty-span-kept'] += span.start == span.end
        else:
            reached['first-longest-dropped'] += 1
    return sorted(kept, key=lambda s: s.start)


def prioritized(existing: list[SpanRecord], spans: list[SpanRecord], keep_existing: bool,
                reached: Counter[str]) -> list[SpanRecord]:
    """spaCy's `prioritize_new_ents_filter` and `prioritize_existing_ents_filter`."""
    order = sorted(spans, key=lambda s: (s.start - s.end, s.start))
    seen: set[int] = set()
    if keep_existing:
        for e in existing:
            seen.update(range(e.start, e.end))
    kept = list(existing)
    added: list[SpanRecord] = []
    for span in order:
        if seen.intersection(range(span.start, span.end)):
            reached['prioritized-dropped'] += 1
            continue
        added.append(span)
        seen.update(range(span.start, span.end))
        if not keep_existing:
            remaining = [e for e in kept if not (e.start < span.end and e.end > span.start)]
            reached['existing-removed'] += len(kept) - len(remaining)
            kept = remaining
    return kept + added


def apply_filter(kind: Filter, existing: list[SpanRecord], spans: list[SpanRecord], reached: Counter[str]) -> list[SpanRecord]:
    reached[f'filter-{kind}'] += 1
    if kind == 'first_longest':
        return first_longest(existing, spans, reached)
    return prioritized(existing, spans, kind == 'prioritize_existing', reached)


def groups(before: Document, matches: list[SpanRecord], options: Options, reached: Counter[str]) -> list[Group]:
    """spaCy's `SpanRuler.set_annotations` for `doc.spans`."""
    key = options.spans_key
    if not key:
        reached['no-spans-key'] += 1
        return before.span_groups
    current = next((g for g in before.span_groups if g.key == key), None)
    base = current if current is not None and not options.overwrite else None
    existing = base.spans if base is not None else []
    added = matches if options.spans_filter is None else apply_filter(options.spans_filter, existing, matches, reached)
    if base is not None:
        # The group is extended in place, keeping its name and attrs.
        reached['group-extended'] += 1
        reached['existing-repeated'] += sum(s in existing for s in added)
        group = replace(base, spans=existing + added)
    else:
        reached['attrs-dropped'] += current is not None and bool(current.attrs)
        group = Group(key, key, {}, list(added))
    if current is None:
        return before.span_groups + [group]
    reached['key-position-kept'] += before.span_groups[-1].key != key
    return [group if g.key == key else g for g in before.span_groups]


def entities(before: Document, matches: list[SpanRecord], options: Options,
             reached: Counter[str]) -> tuple[list[EntityRecord] | None, list[str | None], list[tuple[str | None, str | None]]]:
    """spaCy's `SpanRuler.set_annotations` for `doc.ents`, then the `Doc.ents` setter."""
    tokens = before.tokens
    if not options.annotate_entities:
        return before.entities, [t.entity_id for t in tokens], [(t.entity_iob, t.entity_type) for t in tokens]
    existing = [] if options.overwrite else [SpanRecord(e.start, e.end, e.label, e.id) for e in before.entities or []]
    chosen = apply_filter(options.entities_filter, existing, matches, reached)
    final = sorted((s for s in chosen if s.label), key=lambda s: s.start)
    reached['empty-label-dropped'] += sum(not s.label for s in chosen)
    ids = [t.entity_id for t in tokens]
    tags: list[tuple[str | None, str | None]] = [('O', '')] * len(tokens)
    for e in final:
        for i in range(e.start, e.end):
            tags[i] = ('B' if i == e.start else 'I', e.label)
            # Only an entity with an ID writes it; other tokens keep theirs.
            ids[i] = e.id or ids[i]
    reached['missing-made-outside'] += sum(t.entity_iob is None and tags[i][0] == 'O' for i, t in enumerate(tokens))
    reached['inherited-id'] += sum(e.id is None and ids[e.start] is not None for e in final)
    return [EntityRecord(e.start, e.end, e.label, ids[e.start]) for e in final], ids, tags


def annotate(before: Document, matches: list[SpanRecord], options: Options, reached: Counter[str]) -> Document:
    spans, ids, tags = entities(before, matches, options, reached)
    tokens = [replace(t, entity_iob=iob, entity_type=kind, entity_id=ids[i])
              for i, (t, (iob, kind)) in enumerate(zip(before.tokens, tags, strict=True))]
    return Document(before.text, tokens, spans, before.sentences, before.noun_chunks, groups(before, matches, options, reached))


class SpanRulerReferenceTests(unittest.TestCase):
    def test_fixture_regenerates_and_agrees_with_independent_annotation(self) -> None:
        fixture = build()
        frozen = json.loads((ROOT / 'fixtures/span-ruler-v1.expected.json').read_text())
        self.assertEqual(asdict(fixture), frozen)
        strings = spacy.blank('en').vocab.strings
        reached: Counter[str] = Counter()
        for case in fixture.cases:
            previous = case.document
            for index, (step, outcome) in enumerate(zip(case.steps, case.outcomes, strict=True)):
                at = f'{case.id} step {index}'
                if outcome.error:
                    reached[f'error-{step.action}'] += 1
                if step.action == 'remove' and outcome.error is None and index > 0:
                    # Labels stay known after their patterns are removed, so removing them again succeeds.
                    reached['removed-known-label'] += case.outcomes[index - 1].length == outcome.length
                if step.action == 'remove_by_id' and step.argument == '' and outcome.error is None:
                    reached['remove-empty-id'] += 1
                if step.action != 'apply':
                    self.assertIsNone(outcome.matches, at)
                    continue
                matches = outcome.matches
                result = outcome.result
                if matches is None or result is None:
                    self.fail(f'{at}: an application records matches and a result')
                source = case.document if step.input == 'initial' else previous
                # Spans sort by position, then by the string-store hashes of label and ID.
                keys = [(m.start, m.end, strings.add(m.label), strings.add(m.id or '')) for m in matches]
                self.assertEqual(keys, sorted(set(keys)), f'{at}: sorted without duplicates')
                reached['same-span-tie'] += len({(m.start, m.end) for m in matches}) < len(matches)
                self.assertTrue(all(m.end > m.start for m in matches), at)
                self.assertEqual(annotate(source, matches, case.options, reached), result, at)
                previous = result
        expected = ['filter-first_longest', 'filter-prioritize_new', 'filter-prioritize_existing', 'first-longest-dropped',
                    'prioritized-dropped', 'existing-removed', 'empty-span-kept', 'no-spans-key', 'group-extended',
                    'existing-repeated', 'attrs-dropped', 'key-position-kept', 'empty-label-dropped', 'missing-made-outside',
                    'inherited-id', 'error-remove', 'error-remove_by_id', 'removed-known-label', 'remove-empty-id', 'same-span-tie']
        for behaviour in expected:
            self.assertGreater(reached[behaviour], 0, behaviour)


if __name__ == '__main__':
    unittest.main()
