"""Frozen entity ID fixture checks with an independent recomputation of every state."""
from dataclasses import asdict
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from entity_id_reference import Case, EntitySpan, State, TokenEntity, Update, build
from entity_update_reference import ROOT


def expected_state(before: State, update: Update) -> State:
    """Apply `Doc.set_ents` with entity IDs using plain lists, without spaCy."""
    length = len(before.tokens)
    labeled = [e for e in update.entities if e.label]
    unlabeled = [*update.blocked, *update.missing, *update.outside]
    ranges = [(e.start, e.end) for e in update.entities] + [(r.start, r.end) for r in unlabeled]
    if any(not 0 <= start <= end <= length for start, end in ranges):
        return State('IndexError', before.tokens, before.entities)
    covered = [i for e in labeled for i in range(e.start, e.end)] + [i for r in unlabeled for i in range(r.start, r.end)]
    if len(covered) != len(set(covered)):
        return State('ValueError', before.tokens, before.entities)
    tags: list[tuple[str | None, str]] = [(t.iob, t.type or '') for t in before.tokens]
    # IDs change only where an entity carries one; every other token keeps its ID.
    ids = [t.id for t in before.tokens]
    for e in labeled:
        for i in range(e.start, e.end):
            tags[i] = ('B' if i == e.start else 'I', e.label)
            ids[i] = e.id or ids[i]
    for ranges_, iob in ((update.blocked, 'B'), (update.missing, None), (update.outside, 'O')):
        for r in ranges_:
            for i in range(r.start, r.end):
                tags[i] = (iob, '')
    default = {'outside': 'O', 'missing': None, 'blocked': 'B'}
    if update.default != 'unmodified':
        for i in set(range(length)) - set(covered):
            tags[i] = (default[update.default], '')
    for i in range(1, length):
        (previous, previous_type), (iob, kind) = tags[i - 1], tags[i]
        if iob == 'I' and (previous in (None, 'O') or previous_type != kind):
            tags[i] = ('B', kind)
    tokens = [TokenEntity(iob, kind if iob else None, ids[i]) for i, (iob, kind) in enumerate(tags)]
    if tokens and all(t.iob is None for t in tokens):
        return State(None, tokens, None)
    entities: list[EntitySpan] = []
    for i, (iob, kind) in enumerate(tags):
        if iob == 'B' and kind:
            end = i + 1
            while end < length and tags[end][0] == 'I':
                end += 1
            # The getter reads an entity's ID from its first token.
            entities.append(EntitySpan(i, end, kind, ids[i]))
    return State(None, tokens, entities)


class EntityIdReferenceTests(unittest.TestCase):
    def test_fixture_regenerates_and_agrees_with_independent_states(self) -> None:
        fixture = build()
        self.assertEqual(asdict(fixture), json.loads((ROOT / 'fixtures/entity-ids-v1.expected.json').read_text()))
        cases: list[Case] = fixture.cases
        # Behaviours the suite must reach: an entity inheriting a stale ID, IDs kept on every
        # non-entity state, and both spaCy errors.
        reached = {'inherited': 0, 'kept-outside': 0, 'kept-missing': 0, 'kept-blocked': 0, 'ignored-unlabeled-id': 0,
                   'IndexError': 0, 'ValueError': 0}
        for case in cases:
            current = case.initial
            for step, (update, recorded) in enumerate(zip(case.updates, case.states, strict=True)):
                self.assertEqual(expected_state(current, update), recorded, f'{case.id} update {step}')
                if recorded.error:
                    reached[recorded.error] += 1
                else:
                    for e in recorded.entities or []:
                        reached['inherited'] += any(u.start == e.start and u.label and not u.id and e.id for u in update.entities)
                    for u in update.entities:
                        reached['ignored-unlabeled-id'] += bool(not u.label and u.id and any(
                            recorded.tokens[i].id != u.id for i in range(u.start, u.end)))
                    for token in recorded.tokens:
                        if token.id and not token.type:
                            state = {'O': 'kept-outside', None: 'kept-missing', 'B': 'kept-blocked'}[token.iob]
                            reached[state] += 1
                current = recorded
        self.assertTrue(all(reached.values()), reached)

    def test_generator_refuses_to_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'entity-ids.json'
            output.write_text('frozen')
            result = subprocess.run([sys.executable, str(ROOT / 'tools/entity_id_reference.py'), str(output)],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Refusing to overwrite', result.stderr)
            self.assertEqual(output.read_text(), 'frozen')


if __name__ == '__main__':
    unittest.main()
