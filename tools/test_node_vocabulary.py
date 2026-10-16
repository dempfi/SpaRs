"""Check the Node vocabulary types against spaCy, the model metadata, the catalog and the native code."""
from pathlib import Path
import re
import unittest

from spacy.parts_of_speech import IDS
from spacy.pipeline.span_ruler import DEFAULT_SPANS_KEY as SPAN_RULER_KEY
from spacy.pipeline.spancat import DEFAULT_SPANS_KEY as SPANCAT_KEY
from spacy.pipeline.span_finder import DEFAULT_SPANS_KEY as SPAN_FINDER_KEY

from json_types import json_array, json_object, json_string, read_json

ROOT = Path(__file__).resolve().parent.parent
NODE = ROOT / 'bindings/node'
VOCABULARY = (NODE / 'scripts/vocabulary.d.ts').read_text()
MODELS = ['en_core_web_sm', 'en_core_web_md', 'en_core_web_lg']


def matches(pattern: str, text: str, flags: int = 0) -> list[str]:
    """The first group of every match, failing when there is none so a format change cannot pass silently."""
    values: list[str] = []
    for found in re.finditer(pattern, text, flags):
        value = found.group(1)
        assert isinstance(value, str), pattern
        values.append(value)
    assert values, pattern
    return values


def block(pattern: str, text: str) -> str:
    found = re.search(pattern, text, re.S)
    assert found is not None, pattern
    value = found.group(1)
    assert isinstance(value, str), pattern
    return value


def union(name: str) -> list[str]:
    """The string literals of an exported union type: its first line and the indented lines that follow."""
    return matches(r"'([^']*)'", block(rf'export type {name} =([^\n]*(?:\n[ \t][^\n]*)*)', VOCABULARY))


def registry(name: str) -> list[str]:
    """The keys of an exported registry interface; every key is quoted or a bare identifier."""
    body = block(rf'export interface {name} \{{(.*?)\n\}}', VOCABULARY)
    keys = matches(r"""(?:^|[\s{;])('[^']*'|"[^"]*"|[A-Za-z_$][\w$]*): true""", body)
    return [key[1:-1] if key[0] in '\'"' else key for key in keys]


def model_labels(component: str) -> set[str]:
    labels: set[str] = set()
    for model in MODELS:
        metadata = json_object(json_object(read_json(ROOT / f'assets/{model}-3.8.0/manifest.json'))['metadata'])
        labels.update(json_string(label) for label in json_array(json_object(metadata['labels'])[component]))
    return labels


def rust_strings(path: str, pattern: str) -> list[str]:
    return matches(pattern, (ROOT / path).read_text())


class NodeVocabularyTests(unittest.TestCase):
    def test_pos_tags_match_spacy_and_snapshot_validation(self) -> None:
        expected = sorted(name for name in IDS if name)
        self.assertEqual(sorted(union('UniversalPos')), expected)
        native = block(r'UNIVERSAL_POS: \[&str; \d+\] = \[(.*?)\];', (ROOT / 'crates/spars/src/document.rs').read_text())
        self.assertEqual(sorted(matches(r'"([^"]+)"', native)), expected)

    def test_known_labels_are_the_supported_models_labels(self) -> None:
        self.assertEqual(set(registry('EntityLabels')), model_labels('ner'))
        self.assertEqual(set(registry('FineGrainedTags')), model_labels('tagger'))
        self.assertEqual(set(registry('DependencyLabels')), model_labels('parser'))

    def test_span_group_keys_are_spacy_component_defaults(self) -> None:
        self.assertEqual(SPAN_FINDER_KEY, SPANCAT_KEY)
        self.assertEqual(sorted(registry('SpanGroupKeys')), sorted({SPAN_RULER_KEY, SPANCAT_KEY}))

    def test_model_names_match_the_catalog(self) -> None:
        catalog = sorted({json_string(json_object(entry)['model']) for entry in json_array(read_json(ROOT / 'models/catalog.json'))})
        self.assertEqual(sorted(union('OfficialModelName')), catalog)
        self.assertEqual(sorted(registry('ModelNames')), catalog)
        names = block(r'OFFICIAL_MODEL_NAMES = new Set\(\[(.*?)\]\)', (NODE / 'execution.cjs').read_text())
        self.assertEqual(sorted(matches(r"'([^']+)'", names)), catalog)

    def test_matcher_vocabularies_match_the_native_parsers(self) -> None:
        phrase = rust_strings('bindings/node/src/phrase_matcher.rs', r'\("([A-Z_]+)", spars::PhraseAttribute::')
        self.assertEqual(union('PhraseAttribute'), phrase)
        attributes = rust_strings('bindings/node/src/matcher_types.rs', r'\n\s*"([a-z_]+)" => spars::TokenAttribute::')
        declared = union('StringAttribute') + union('FlagAttribute') + ['pos', 'tag', 'dep', 'morphology', 'length']
        self.assertEqual(sorted(declared), sorted(attributes))
        relations = rust_strings('bindings/node/src/dependency_matcher.rs', r'\n\s*"([^"]+)" => [A-Z]\w+,')
        self.assertEqual(sorted(union('DependencyRelation')), sorted(relations))
        # The core enum's serde names are the relations native patterns accept.
        native_relations = rust_strings('crates/spars/src/dependency_matcher/relations.rs', r'#\[serde\(rename = "([^"]+)"\)\]')
        self.assertEqual(sorted(union('DependencyRelation')), sorted(native_relations))
        operators = rust_strings('bindings/node/src/matcher_types.rs', r'\n\s*"([^"]+)" => spars::Comparison::')
        self.assertEqual(sorted(union('Comparison')), sorted(operators))
        repetitions = rust_strings('bindings/node/src/token_matcher.rs', r'"([a-z_]+)" => spars::Repetition::')
        repetitions += rust_strings('bindings/node/src/token_matcher.rs', r'kind == "(range)"')
        declared_repetitions = block(r'export type TokenRepetition =(.*?)\n\n', VOCABULARY)
        self.assertEqual(sorted(set(matches(r"kind: '([a-z_]+)'", declared_repetitions) + matches(r"\| '([a-z_]+)'", declared_repetitions))), sorted(set(repetitions)))
        # Every single-word string in the predicate kind match; error messages contain spaces.
        kinds = matches(r'"([a-z_]+)"', block(r'let predicate = match kind\.as_str\(\) \{(.*?)Ok\(spars::TokenConstraint', (NODE / 'src/matcher_types.rs').read_text()))
        constraint = block(r'export type TokenConstraint =(.*?)\n\n', VOCABULARY)
        string_predicate = block(r'export type StringPredicate<[^>]*> =(.*?)\n\n', VOCABULARY)
        declared_kinds = union('StringSetKind') + union('IntegerSetKind') + matches(r"kind: '([a-z_]+)'", constraint)
        declared_kinds += matches(r"kind: '([a-z_]+)'", string_predicate)
        declared_kinds += matches(r"'(morph_[a-z]+)'", constraint)
        self.assertEqual(sorted(set(declared_kinds)), sorted(set(kinds)))

    def test_error_codes_match_every_raised_code(self) -> None:
        # Only raising sites count: the native error mapping and `code:` properties in the wrapper.
        raised = set(rust_strings('bindings/node/src/errors.rs', r'=> "(SPARS_[A-Z_]+)"'))
        raised |= set(matches(r"code: '(SPARS_[A-Z_]+)'", (NODE / 'execution.cjs').read_text()))
        self.assertEqual(set(union('SparsErrorCode')), raised)

    def test_vocabulary_has_no_duplicates(self) -> None:
        for name in ['UniversalPos', 'PhraseAttribute', 'DependencyRelation', 'SparsErrorCode', 'OfficialModelName']:
            values = union(name)
            self.assertEqual(len(values), len(set(values)), name)
        for name in ['EntityLabels', 'FineGrainedTags', 'DependencyLabels', 'ModelNames']:
            keys = registry(name)
            self.assertEqual(len(keys), len(set(keys)), name)


if __name__ == '__main__':
    unittest.main()
