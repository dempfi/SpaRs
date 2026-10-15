# Edit entity annotation

`Doc::set_entities` replaces a document's named-entity annotation, following spaCy 3.8.14's `Doc.set_ents`. Use it to add or correct entities after the recognizer has run, or to annotate a document that never ran it. Each token's IOB tag (`B` begins an entity, `I` continues it, `O` is outside any entity), each token's entity type and the document's entity list stay consistent after every update.

`set_entities` itself needs no model. The example below processes text first, so it needs the `en_core_web_md` export at `assets/en_core_web_md-3.8.0`, relative to the repository root, from the [development setup](DEVELOPMENT.md) or the [model installer](MODEL_INSTALLATION.md).

```rust
use spars::{EntityDefault, EntityUpdate, Model, Span, TokenIndex, TokenRange};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nlp = Model::load("assets/en_core_web_md-3.8.0")?;
    let mut doc = nlp.process("Tim Cook visited London.")?;
    let labels = |doc: &spars::Doc| -> Vec<String> {
        doc.entities().unwrap().iter().map(|e| e.label.clone()).collect()
    };
    assert_eq!(labels(&doc), ["PERSON", "GPE"]);

    // Relabel London and keep the other predictions.
    doc.set_entities(&EntityUpdate {
        entities: vec![Span::new(TokenIndex(3), TokenIndex(4), "CITY")],
        default: EntityDefault::Unmodified,
        ..EntityUpdate::default()
    })?;
    assert_eq!(labels(&doc), ["PERSON", "CITY"]);
    assert_eq!(doc.tokens()[3].entity_iob.as_deref(), Some("B"));

    // Spans that share a token are rejected and the document is unchanged.
    let overlapping = EntityUpdate {
        entities: vec![Span::new(TokenIndex(0), TokenIndex(2), "ORG")],
        outside: vec![TokenRange { start: TokenIndex(1), end: TokenIndex(3) }],
        ..EntityUpdate::default()
    };
    assert!(matches!(doc.set_entities(&overlapping), Err(spars::Error::Bounds)));
    assert_eq!(labels(&doc), ["PERSON", "CITY"]);
    Ok(())
}
```

With `Unmodified`, "Tim Cook" keeps its predicted `PERSON` label while London becomes `CITY`. The second update would mark "Cook" both as part of an `ORG` entity and as outside any entity, so it is rejected and `CITY` stays in place.

## Token states

An `EntityUpdate` lists half-open token intervals (the end index is excluded) in four groups. Each group sets one state on its tokens:

| Group | Token IOB tag | Token entity type | Meaning |
| --- | --- | --- | --- |
| `entities` | `B` on the first token, `I` on the rest | The span label | A named entity |
| `outside` | `O` | `""` | Known not to be part of an entity |
| `blocked` | `B` | `""` | Can never be part of an entity. spaCy's recognizer respects this when it runs afterwards; SpaRs cannot yet run its recognizer on an existing document (planned as delivery A4 in the [progress plan](PROGRESS.md#2-checked-annotations-and-rule-based-annotation)), so here it only records the state |
| `missing` | `None` | `None` | Unknown, as before entity recognition |

`default` sets every token outside all four groups: `Outside` (the default), `Missing`, `Blocked`, or `Unmodified` to keep its current state. A token in the `entities` group ends up in `Doc::entities`; the other states never do. An entity with an empty label is ignored, as in spaCy, but its interval must still be inside the document. The recognizer itself writes `O` with an empty entity type, so an updated document and a predicted one represent outside tokens in the same way.

`Doc::entities` is `None` when a non-empty document has every token missing, meaning it has no entity annotation, which corresponds to spaCy's `doc.has_annotation("ENT_IOB")` being false. Otherwise it is the list of entities, which may be empty. An empty document always has an empty list, as spaCy counts it as annotated and `Model::process("")` returns. spaCy's `doc.ents` itself is an empty tuple in both cases.

After writing the groups and the default, an `I` tag that follows a missing or `O` token, or a token of a different type, becomes `B`. For example, marking the middle of a three-token entity as missing with `Unmodified` leaves two separate one-token entities. An `I` that follows a token of the same type is kept, so relabeling only the first token of a three-token `X` entity as `X` leaves one three-token entity. This is spaCy's repair step, applied in token order.

Tokens that the update neither writes nor repairs keep their stored values exactly. This only matters for documents restored from a native snapshot (the JSON written by `Doc::to_json` and read by `Doc::from_json`), which may spell a missing tag as `""` or store a type with it.

## Validation and errors

Every check runs before anything changes, so a rejected update leaves the document exactly as it was:

- An interval whose end is before its start or past the last token returns `Error::Bounds`. In spaCy, building such a `Span` raises `IndexError`.
- Two intervals that share a token return `Error::Bounds`. Entities with an empty label are left out of this check, and empty intervals share no tokens. spaCy raises `ValueError` (E1010).
- An `I` on the first token that the update leaves in place returns `Error::Unsupported`, because there is no entity for it to continue; any other `I` without a `B` before it is repaired as described above. Model output and earlier updates never contain this tag; only an edited native snapshot can. A current IOB tag other than `B`, `I`, `O` or missing also returns `Error::Unsupported`; `Doc::from_json` already rejects such tags, so only code that sets `Token::entity_iob` directly can produce one. For the first-token case, spaCy writes the update and raises `ValueError` (E093) when the entity list is next read; SpaRs rejects the update without changing the document.
- When deserializing an `EntityUpdate` from JSON, unknown fields, including spaCy's `kb_id`, are rejected rather than ignored.

## Entity IDs

An entity can carry an ID in addition to its label. The ID is a string you choose, usually a stable identifier for the thing the entity names; for example, both "Tim Cook" and "Mr. Cook" can have the ID `"tim-cook"`. Set `Span::id` on an entity in the update; `set_entities` writes it to every token of the entity (`Token::entity_id`), and `Doc::entities` reports each entity's ID from its first token. These are spaCy's `Span.id_` and `Token.ent_id_`. The named-entity recognizer never sets IDs, so predicted entities have none until an update adds one.

This example needs no model: it builds a three-token document from a minimal native snapshot (see `Doc::from_json`). Documents from `Model::process` work the same way.

```rust
use spars::{Doc, EntityUpdate, Span, TokenIndex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = r#"{"format_version":1,"document":{"text":"Tim Cook spoke","tokens":[
        {"start":0,"end":3,"idx":0,"whitespace":true,"norm":"tim"},
        {"start":4,"end":8,"idx":4,"whitespace":true,"norm":"cook"},
        {"start":9,"end":14,"idx":9,"whitespace":false,"norm":"spoke"}]}}"#;
    let mut doc = Doc::from_json(snapshot)?;
    let mut person = Span::new(TokenIndex(0), TokenIndex(2), "PERSON");
    person.id = Some("tim-cook".into());
    doc.set_entities(&EntityUpdate { entities: vec![person], ..EntityUpdate::default() })?;
    assert_eq!(doc.entities().unwrap()[0].id.as_deref(), Some("tim-cook"));

    // An entity without an ID keeps the ID its tokens already have, as in spaCy.
    let relabeled = Span::new(TokenIndex(0), TokenIndex(2), "CEO");
    doc.set_entities(&EntityUpdate { entities: vec![relabeled], ..EntityUpdate::default() })?;
    assert_eq!(doc.entities().unwrap()[0].id.as_deref(), Some("tim-cook"));

    // Tokens that stop being part of an entity keep their ID too.
    doc.set_entities(&EntityUpdate::default())?;
    assert_eq!(doc.entities().unwrap().len(), 0);
    assert_eq!(doc.tokens()[0].entity_id.as_deref(), Some("tim-cook"));
    Ok(())
}
```

IDs follow spaCy's rules, including two that can surprise:

- An entity without an ID, or with an empty one, does not clear IDs. Each of its tokens keeps its current ID, so a relabeled entity keeps its old ID, and a new entity without an ID reports the ID its first token already had. Give the entity an ID to replace it.
- Outside, missing and blocked tokens keep their IDs. Only `Token::entity_id` shows such an ID; `Doc::entities` never includes those tokens.

In an update, an empty ID means the same as `None`, as in spaCy. Native snapshots use format version 3 when any token or entity has an ID; documents without IDs are still written as version 1 or 2, and older SpaRs versions reject version 3 instead of silently dropping the IDs. `Doc::from_json` rejects an empty ID, an ID on a sentence or noun chunk, and an entity whose ID differs from its first token's, since spaCy cannot represent those. Knowledge-base IDs (`kb_id`) are not stored; see [limits](#limits).

## Ownership and views

`set_entities` takes `&mut Doc`. Rust's borrow rules prevent calling it while a `TokenView`, `SpanView` or other borrow of the document is alive, so no view can observe a half-applied update. To keep the earlier annotation, call `doc.with_entities(&update)` instead: it checks the update, then returns an updated copy and leaves the original unchanged, so an invalid update copies nothing. Text, token boundaries, offsets and all other annotations are unchanged. A native snapshot taken after an update restores the same annotation.

The [Node binding](NODE.md#edit-entity-annotation) offers only the copying form, `withEntities`, because native documents and their views are immutable and shared.

## Limits

These differ from spaCy or are not supported:

- Knowledge-base IDs (`kb_id`) are not stored. spaCy's `set_ents` copies each entity span's knowledge-base ID to its tokens; SpaRs has no entity linker to produce them, and they have no planned delivery. [Entity IDs](#entity-ids) are stored.
- spaCy's `Doc.ents` setter, which also accepts `(label, start, end)` tuples and tuples carrying knowledge-base and entity IDs, is not provided; nor is the `ents` argument of spaCy's `Doc` constructor, which takes IOB strings. Use `set_entities` with token intervals.
- spaCy's `set_ents` edits the document in place; `Doc::with_entities` and the Node binding return a copy, as described above.
- Span groups (`Doc.spans`), merging or splitting tokens, and other annotation edits are not supported. The [compatibility inventory](COMPATIBILITY.md) tracks them.

## Verification

`fixtures/entity-updates-v1.expected.json` records spaCy 3.8.14's token tags, entity list and errors after 330 updates in 34 cases. One case applies four updates to `en_core_web_md` 3.8.0 predictions; the others use constructed documents. `crates/spars/tests/entity_updates.rs` replays every update through both `set_entities` and `with_entities` and checks that rejected updates change nothing; its model-dependent test checks that SpaRs predicts that case's starting entities and that updating them leaves all other annotations unchanged. `crates/spars/tests/entity_update_allocation.rs` checks that a rejected `with_entities` update does not copy the document. `bindings/node/tests/entities.test.mts` replays the same fixture through `withEntities`. `tools/test_entity_update_reference.py` regenerates the fixture with spaCy and then recomputes every recorded state from the rules above with plain Python. `fixtures/entity-ids-v1.expected.json` separately records spaCy's tags, types, token IDs and entity IDs after 193 updates (2 rejected) in 27 cases; its one model case starts from spaCy's recorded predictions, so replaying it needs no model. `crates/spars/tests/entity_ids.rs` and `bindings/node/tests/entity-ids.test.mts` replay it, and `tools/test_entity_id_reference.py` regenerates the fixture with spaCy, compares it with the frozen file, and recomputes every state with plain Python. Run them with:

```sh
cargo test --release --offline -p spars-nlp --test entity_updates --test entity_ids --test entity_update_allocation -- --include-ignored
npm --prefix bindings/node test
.venv/bin/python -m unittest discover -s tools -p 'test_entity_*_reference.py'
```

The model-dependent tests need the exported models; see [validation](VALIDATION.md) for the full acceptance run.
