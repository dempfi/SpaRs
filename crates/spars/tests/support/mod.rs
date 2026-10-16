//! Helpers shared by integration tests that restore recorded reference documents.
use spars::{Span, Token, TokenIndex};

/// The entities spaCy's `doc.ents` reads from recorded token tags: a beginning tag with a type
/// starts an entity, inside tags continue it, and an entity's ID is its first token's. The token
/// and dependency matcher generators record the pipeline's tags but write `[]` in place of the
/// entity list, which `Doc::from_json` rejects when the tags describe entities.
pub fn entities_from_tags(tokens: &[Token]) -> Vec<Span> {
    let entity = |start: usize, end: usize| {
        let mut span = Span::new(
            TokenIndex(start),
            TokenIndex(end),
            tokens[start].entity_type.clone().unwrap_or_default(),
        );
        span.id = tokens[start].entity_id.clone();
        span
    };
    let mut spans = Vec::new();
    let mut open = None;
    for (i, token) in tokens.iter().enumerate() {
        if token.entity_iob.as_deref() == Some("I") {
            continue;
        }
        spans.extend(open.take().map(|start| entity(start, i)));
        let typed = token.entity_type.as_deref().is_some_and(|t| !t.is_empty());
        if token.entity_iob.as_deref() == Some("B") && typed {
            open = Some(i);
        }
    }
    spans.extend(open.map(|start| entity(start, tokens.len())));
    spans
}
