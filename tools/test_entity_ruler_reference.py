"""Frozen EntityRuler fixture checks with an independent recomputation of every annotation."""
from dataclasses import asdict, replace
import json
import unittest

from entity_ruler_reference import Document, EntityMatch, EntityRecord, Options, build
from entity_update_reference import ROOT


def annotate(before: Document, matches: list[EntityMatch], options: Options, reached: dict[str, int]) -> Document:
    """spaCy's `EntityRuler.set_annotations` and `Doc.ents` setter, with plain lists."""
    tokens = before.tokens
    entities = list(before.entities or [])
    added: list[EntityRecord] = []
    covered: set[int] = set()
    for m in matches:
        span = range(m.start, m.end)
        if any(tokens[i].entity_type for i in span) and not options.overwrite_entities:
            reached['skipped-existing'] += 1
            continue
        if covered.intersection(span):
            reached['dropped-overlap'] += 1
            reached['dropped-match-over-kept-entity'] += any(
                e.start < m.end and e.end > m.start and not covered.intersection(range(e.start, e.end)) for e in entities)
            continue
        if any(tokens[i].entity_iob == 'B' and not tokens[i].entity_type for i in span):
            reached['blocked-accepted'] += 1
        kept = [e for e in entities if not (e.start < m.end and e.end > m.start)]
        reached['removed-existing'] += len(entities) - len(kept)
        reached['removed-several-by-one-match'] += len(entities) - len(kept) > 1
        entities = kept
        covered.update(span)
        if not m.label:
            # Setting the entities drops a span without a label after it has claimed its tokens.
            reached['empty-label-dropped'] += 1
            continue
        added.append(EntityRecord(m.start, m.end, m.label, m.id))
    final = sorted(entities + added, key=lambda e: e.start)
    ids = [t.entity_id for t in tokens]
    tags: list[tuple[str, str]] = [('O', '')] * len(tokens)
    for e in final:
        for i in range(e.start, e.end):
            tags[i] = ('B' if i == e.start else 'I', e.label)
            # Only an entity with an ID writes it; other tokens keep theirs.
            ids[i] = e.id or ids[i]
    reached['missing-made-outside'] += sum(t.entity_iob is None and tags[i][0] == 'O' for i, t in enumerate(tokens))
    result = [replace(t, entity_iob=iob, entity_type=kind, entity_id=ids[i])
              for i, (t, (iob, kind)) in enumerate(zip(tokens, tags, strict=True))]
    spans = [EntityRecord(e.start, e.end, e.label, ids[e.start]) for e in final]
    reached['inherited-id'] += sum(e.id is None and ids[e.start] is not None for e in added)
    return Document(before.text, result, spans, before.sentences, before.noun_chunks)


class EntityRulerReferenceTests(unittest.TestCase):
    def test_fixture_regenerates_and_agrees_with_independent_annotation(self) -> None:
        fixture = build()
        frozen = json.loads((ROOT / 'fixtures/entity-ruler-v1.expected.json').read_text())
        self.assertEqual(asdict(fixture), frozen)
        reached = {'skipped-existing': 0, 'dropped-overlap': 0, 'blocked-accepted': 0, 'removed-existing': 0,
                   'removed-several-by-one-match': 0, 'dropped-match-over-kept-entity': 0, 'empty-label-dropped': 0,
                   'missing-made-outside': 0, 'inherited-id': 0, 'error': 0, 'stale-id-error': 0}
        for case in fixture.cases:
            previous = case.document
            for index, (step, outcome) in enumerate(zip(case.steps, case.outcomes, strict=True)):
                at = f'{case.id} step {index}'
                if outcome.error:
                    reached['error'] += 1
                    # A removed ID stays known to spaCy, so removing it again fails differently.
                    reached['stale-id-error'] += any(o.error is None and s.action == 'remove' and s.id == step.id
                                                     for s, o in zip(case.steps[:index], case.outcomes[:index]))
                if step.action != 'apply':
                    self.assertIsNone(outcome.matches, at)
                    continue
                matches = outcome.matches
                result = outcome.result
                if matches is None or result is None:
                    self.fail(f'{at}: an application records matches and a result')
                source = case.document if step.input == 'initial' else previous
                lengths = [(m.end - m.start, -m.start) for m in matches]
                self.assertEqual(lengths, sorted(lengths, reverse=True), f'{at}: longest, then earliest first')
                self.assertTrue(all(m.end > m.start for m in matches), at)
                self.assertEqual(annotate(source, matches, case.options, reached), result, at)
                previous = result
        for behaviour, count in reached.items():
            self.assertGreater(count, 0, behaviour)


if __name__ == '__main__':
    unittest.main()
