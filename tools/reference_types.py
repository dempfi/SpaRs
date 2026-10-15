"""Typed interfaces used from the pinned official reference packages.

These protocols describe only the CPU inference and export operations used here.
They do not claim to describe the complete spaCy or Thinc APIs.
"""
from collections.abc import Iterable, Iterator, Mapping, Sequence
from contextlib import AbstractContextManager
from typing import Literal, Protocol, overload, TypedDict, TypeGuard, runtime_checkable
import re
import numpy as np
from numpy.typing import NDArray

FloatArray = NDArray[np.float32]
UIntArray = NDArray[np.uint64]
IntArray = NDArray[np.int32]
from json_types import JsonValue as Json

class RegexFunction(Protocol):
    __self__: re.Pattern[str]
    def __call__(self, text: str) -> re.Match[str] | None: ...

class RegexIterator(Protocol):
    __self__: re.Pattern[str]
    def __call__(self, text: str) -> Iterator[re.Match[str]]: ...

class Span(Protocol):
    start: int
    end: int
    label_: str
    id_: str
    vector: FloatArray

class Token(Protocol):
    text: str
    idx: int
    i: int
    whitespace_: str
    norm_: str
    tag_: str
    pos_: str
    morph: object
    lemma_: str
    head: 'Token'
    dep_: str
    is_sent_start: bool | None
    ent_iob_: str
    ent_type_: str
    ent_id_: str
    vector: FloatArray
    @property
    def children(self) -> Iterator['Token']: ...
    @property
    def ancestors(self) -> Iterator['Token']: ...
    @property
    def subtree(self) -> Iterator['Token']: ...
    @property
    def sent(self) -> Span: ...
    def __len__(self) -> int: ...
    def set_morph(self, features: str) -> None: ...

class Doc(Protocol):
    text: str
    ents: tuple[Span, ...]
    sents: Iterable[Span]
    noun_chunks: Iterable[Span]
    vector: FloatArray
    tensor: FloatArray
    @overload
    def __getitem__(self, index: int) -> Token: ...
    @overload
    def __getitem__(self, index: slice) -> Span: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[Token]: ...
    def to_array(self, attrs: list[int]) -> UIntArray: ...
    def set_ents(self, entities: list[Span], *, blocked: list[Span] | None = None, missing: list[Span] | None = None,
                 outside: list[Span] | None = None, default: str = 'outside') -> None: ...
    def has_annotation(self, attr: str) -> bool: ...
    def similarity(self, other: 'Doc') -> float: ...
    def copy(self) -> 'Doc': ...

class Ops(Protocol):
    def flatten(self, arrays: list[FloatArray], *, pad: int = 0) -> FloatArray: ...

class NamedModel(Protocol):
    name: str

class Model(NamedModel, Protocol):
    param_names: tuple[str, ...]
    attrs: Mapping[str, object]
    layers: list['Model']
    ops: Ops
    def get_param(self, name: str) -> FloatArray: ...
    def get_dim(self, name: str) -> int: ...
    def get_ref(self, name: str) -> 'Model': ...
    def walk(self) -> Iterator['Model']: ...


class State(Protocol):
    def is_final(self) -> bool: ...

@runtime_checkable
class Step(Protocol):
    def predict(self, states: list[State]) -> FloatArray: ...
    def get_token_ids(self, states: list[State]) -> IntArray: ...

class Moves(Protocol):
    n_moves: int
    def get_class_name(self, index: int) -> str: ...
    def init_batch(self, docs: list[Doc]) -> list[State]: ...
    def is_valid(self, state: State, action: str) -> bool: ...
    def apply_transition(self, state: State, action: str) -> None: ...

class Component(Protocol):
    def __call__(self, doc: Doc) -> Doc: ...

class Tok2Vec(Component, Protocol):
    model: Model

class Tagger(Tok2Vec, Protocol):
    labels: tuple[str, ...]

class Parser(Component, Protocol):
    model: Model
    moves: Moves
    cfg: Mapping[str, object]

class Lookups(Protocol):
    tables: list[str]
    def get_table(self, name: str) -> Mapping[int | str, Json]: ...

class Lemmatizer(Component, Protocol):
    lookups: Lookups

class Ruler(Component, Protocol):
    patterns: list[dict[str, Json]]

class Strings(Protocol):
    def __getitem__(self, key: int) -> str: ...

class Vectors(Protocol):
    data: FloatArray
    key2row: Mapping[int, int]
    mode: str
    attr: int

class Lexeme(Protocol):
    orth: int
    norm_: str
    shape_: str
    prefix_: str
    suffix_: str
    vector: FloatArray
    has_vector: bool
    is_lower: bool
    is_upper: bool
    is_title: bool
    is_ascii: bool
    is_currency: bool
    is_stop: bool
    is_bracket: bool
    is_quote: bool
    is_left_punct: bool
    is_right_punct: bool
    like_url: bool
    like_email: bool

class Vocab(Protocol):
    strings: Strings
    vectors: Vectors
    lookups: Lookups
    def __getitem__(self, text: str) -> Lexeme: ...

class Tokenizer(Protocol):
    def __call__(self, text: str) -> Doc: ...
    rules: dict[str, list[dict[int, str]]]
    prefix_search: RegexFunction
    suffix_search: RegexFunction
    infix_finditer: RegexIterator
    url_match: RegexFunction
    token_match: RegexFunction | None
    faster_heuristics: bool

class Config(Protocol):
    def to_str(self) -> str: ...

class Defaults(Protocol):
    stop_words: set[str]

class Language(Protocol):
    meta: dict[str, Json]
    pipe_names: list[str]
    config: Config
    tokenizer: Tokenizer
    vocab: Vocab
    Defaults: Defaults
    def make_doc(self, text: str) -> Doc: ...
    def __call__(self, text: str | Doc) -> Doc: ...
    def add_pipe(self, factory_name: str, name: str = ..., *, config: Mapping[str, Json] = ...) -> object: ...
    def remove_pipe(self, name: str) -> tuple[str, object]: ...
    def select_pipes(self, *, disable: list[str]) -> AbstractContextManager[object]: ...
    @overload
    def get_pipe(self, name: Literal['tok2vec']) -> Tok2Vec: ...
    @overload
    def get_pipe(self, name: Literal['tagger']) -> Tagger: ...
    @overload
    def get_pipe(self, name: Literal['parser', 'ner']) -> Parser: ...
    @overload
    def get_pipe(self, name: Literal['attribute_ruler']) -> Ruler: ...
    @overload
    def get_pipe(self, name: Literal['lemmatizer']) -> Lemmatizer: ...


def integer(value: object) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f'Expected an integer, received {type(value).__name__}')
    return value


def sequence(value: object) -> TypeGuard[Sequence[object]]:
    return isinstance(value, (list, tuple))


def strings(value: object) -> list[str]:
    if not sequence(value):
        raise TypeError('Expected string sequence')
    result: list[str] = []
    for item in value:
        if not isinstance(item, str):
            raise TypeError('Expected string sequence member')
        result.append(item)
    return result


def float_rows(array: FloatArray) -> list[list[float]]:
    return [[float(value) for value in row] for row in array]


def float_values(array: FloatArray) -> list[float]:
    return [float(value) for value in array]


def uint_rows(array: UIntArray) -> list[list[int]]:
    return [[int(value) for value in row] for row in array]


class SpanRecord(TypedDict):
    start: int
    end: int
    label: str

class TokenRecord(TypedDict):
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
    entity_iob: str
    entity_type: str


def span_records(spans: Iterable[Span]) -> list[SpanRecord]:
    return [{'start': s.start, 'end': s.end, 'label': s.label_} for s in spans]


def token_records(doc: Doc, text: str) -> list[TokenRecord]:
    return [{'start': len(text[:t.idx].encode()), 'end': len(text[:t.idx + len(t)].encode()),
             'idx': t.idx, 'whitespace': bool(t.whitespace_), 'norm': t.norm_,
             'tag': t.tag_, 'pos': t.pos_, 'morphology': str(t.morph), 'lemma': t.lemma_,
             'head': t.head.i, 'dep': t.dep_, 'sentence_start': t.is_sent_start,
             'entity_iob': t.ent_iob_, 'entity_type': t.ent_type_} for t in doc]


@runtime_checkable
class Predictor(Protocol):
    def predict(self, inputs: list[Doc] | FloatArray) -> object: ...


def raw_prediction(model: NamedModel, inputs: list[Doc] | FloatArray) -> object:
    if not isinstance(model, Predictor):
        raise TypeError(f'Model {model.name} has no predict method')
    return model.predict(inputs)


def is_ndarray(value: object) -> TypeGuard[NDArray[np.generic]]:
    return isinstance(value, np.ndarray)


def is_float_array(value: object) -> TypeGuard[FloatArray]:
    return is_ndarray(value) and value.dtype == np.dtype(np.float32)


def predict_array(model: NamedModel, inputs: FloatArray) -> FloatArray:
    value = raw_prediction(model, inputs)
    if not is_float_array(value) or value.ndim != 2:
        raise TypeError(f'Model {model.name} must produce a two-dimensional float32 array')
    return value


def predict_docs(model: NamedModel, docs: list[Doc]) -> list[FloatArray]:
    value = raw_prediction(model, docs)
    if not sequence(value):
        raise TypeError(f'Model {model.name} must produce document arrays')
    arrays: list[FloatArray] = []
    for item in value:
        if not is_float_array(item) or item.ndim != 2:
            raise TypeError(f'Model {model.name} must produce two-dimensional float32 arrays')
        arrays.append(item)
    if len(arrays) != len(docs):
        raise ValueError('Model output count differs from the document count')
    return arrays


def predict_step(model: NamedModel, docs: list[Doc]) -> Step:
    value = raw_prediction(model, docs)
    if not isinstance(value, Step):
        raise TypeError(f'Model {model.name} must produce a parser step')
    return value
