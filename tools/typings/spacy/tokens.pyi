from collections.abc import Mapping

from json_types import JsonValue
from reference_types import Doc as ReferenceDoc, Span as ReferenceSpan, SpanGroup as ReferenceSpanGroup, Vocab

def Doc(vocab: Vocab, *, words: list[str], spaces: list[bool] = ..., heads: list[int] = ..., deps: list[str] = ..., pos: list[str] = ..., tags: list[str] = ..., lemmas: list[str] = ..., morphs: list[str] = ..., sent_starts: list[bool] = ...) -> ReferenceDoc: ...

def Span(doc: ReferenceDoc, start: int, end: int, label: str = ..., *, span_id: str = ...) -> ReferenceSpan: ...

def SpanGroup(doc: ReferenceDoc, *, name: str = ..., attrs: Mapping[str, JsonValue] = ..., spans: list[ReferenceSpan] = ...) -> ReferenceSpanGroup: ...
