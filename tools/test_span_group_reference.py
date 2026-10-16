"""Frozen span group fixture checks with an independent replay of every edit."""
from dataclasses import asdict, dataclass, field
import json
import unittest

from entity_update_reference import ROOT
from json_types import JsonValue
from span_group_reference import Group, SpanRecord, Step, build


@dataclass
class Model:
    """Doc.spans as an ordered dict of plain records."""
    name: str
    attrs: dict[str, JsonValue] = field(default_factory=dict[str, JsonValue])
    spans: list[SpanRecord] = field(default_factory=list[SpanRecord])


def overlaps(spans: list[SpanRecord]) -> bool:
    """Any two spans sharing a token, or an empty span strictly inside another span's bounds."""
    bounds = sorted((s.start, s.end) for s in spans)
    return any(b[0] < a[1] for i, a in enumerate(bounds) for b in bounds[i + 1:])


def replay(groups: dict[str, Model], step: Step, length: int, reached: dict[str, int]) -> dict[str, Model]:
    """Apply one edit to a copy of `groups`; raise the exception spaCy raises, checking in Python's evaluation
    order: `doc.spans[k].append(span)` looks up the key first, `doc.spans[k][i] = span` builds the span first."""
    result = {k: Model(g.name, dict(g.attrs), list(g.spans)) for k, g in groups.items()}
    spans = step.spans or []
    def check_bounds() -> None:
        if any(not 0 <= s.start <= s.end <= length for s in spans):
            raise IndexError('span out of bounds')
    key = step.key or ''
    match step.action:
        case 'set':
            check_bounds()
            result[key] = Model(key, {}, list(spans))
        case 'set_group':
            check_bounds()
            reached['renamed'] += step.name != key
            result[key] = Model(step.name or '', dict(step.attrs or {}), list(spans))
        case 'delete':
            del result[key]
        case 'append' | 'extend':
            group = result[key]
            check_bounds()
            group.spans.extend(spans)
        case 'extend_group' | 'concat':
            if step.action == 'extend_group':
                target, source = result[key], groups[step.other or '']
            else:
                first = groups[step.other or '']
                target, source = Model(first.name, dict(first.attrs), list(first.spans)), groups[step.second or '']
            reached['kept-attr'] += any(k in target.attrs and target.attrs[k] != v for k, v in source.attrs.items())
            # SpanGroup._concat keeps the target's keys first and appends the keys it lacks.
            target.attrs = target.attrs | {k: v for k, v in source.attrs.items() if k not in target.attrs}
            target.spans = target.spans + source.spans
            result[key] = target
        case 'set_item':
            check_bounds()
            group = result[key]
            index = step.index or 0
            if not -len(group.spans) <= index < len(group.spans):
                raise IndexError('index out of range')
            reached['negative-index'] += index < 0
            group.spans[index] = spans[0]
        case 'copy':
            pass
    return result


def snapshot(groups: dict[str, Model]) -> list[Group]:
    return [Group(k, g.name, g.attrs, g.spans, overlaps(g.spans)) for k, g in groups.items()]


class SpanGroupReferenceTests(unittest.TestCase):
    def test_fixture_regenerates_and_agrees_with_independent_replay(self) -> None:
        fixture = build()
        frozen = json.loads((ROOT / 'fixtures/span-groups-v1.expected.json').read_text())
        self.assertEqual(asdict(fixture), frozen)
        reached = {'renamed': 0, 'kept-attr': 0, 'IndexError': 0, 'KeyError': 0, 'overlap': 0, 'reordered-key': 0,
                   'negative-index': 0, 'empty-span': 0, 'span-id': 0, 'readded-key-last': 0, 'attr-order': 0}
        for case in fixture.cases:
            groups: dict[str, Model] = {}
            for index, (step, state) in enumerate(zip(case.steps, case.states, strict=True)):
                at = f'{case.id} step {index}'
                try:
                    after = replay(groups, step, len(case.words), reached)
                    error = None
                except (IndexError, KeyError) as exc:
                    after = groups
                    error = type(exc).__name__
                    reached[error] += 1
                self.assertEqual(state.error, error, at)
                self.assertEqual(state.groups, snapshot(after), at)
                # Dict equality ignores order, so compare attr key order separately.
                self.assertEqual([(g.key, list(g.attrs.items())) for g in state.groups],
                                 [(g.key, list(g.attrs.items())) for g in snapshot(after)], at)
                reached['attr-order'] += any(list(g.attrs) != sorted(g.attrs) for g in state.groups)
                reached['empty-span'] += any(s.start == s.end for g in state.groups for s in g.spans)
                reached['span-id'] += any(s.id for g in state.groups for s in g.spans)
                reached['readded-key-last'] += (error is None and step.action in ('set', 'set_group')
                                                and step.key not in groups and any(
                                                    o.action == 'delete' and o.key == step.key
                                                    for o in case.steps[:index]))
                reached['overlap'] += any(g.has_overlap for g in state.groups)
                # Replacing a key keeps its place even when other keys were added after it.
                reached['reordered-key'] += (error is None and step.action in ('set', 'set_group', 'concat')
                                             and step.key in groups and list(groups)[-1] != step.key)
                groups = after
        for behaviour, count in reached.items():
            self.assertGreater(count, 0, behaviour)


if __name__ == '__main__':
    unittest.main()
