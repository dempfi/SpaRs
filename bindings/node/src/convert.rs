use crate::errors::{number, Result};
use crate::output::{Document, Span, Token};

pub fn document(doc: &spars::Doc) -> Result<Document> {
    // Walk text once in order. Counting each token's prefix would be quadratic.
    let mut byte_cursor = 0;
    let mut utf16_cursor = 0;
    let mut tokens = Vec::with_capacity(doc.tokens().len());
    for (index, token) in doc.tokens().iter().enumerate() {
        utf16_cursor += doc.text()[byte_cursor..token.start.0]
            .encode_utf16()
            .count();
        let text = doc.token_text(spars::TokenIndex(index))?;
        let mut points = 0;
        let mut units = 0;
        for ch in text.chars() {
            points += 1;
            units += ch.len_utf16();
        }
        tokens.push(Token {
            index: number(index)?,
            text: text.to_owned(),
            whitespace: if token.whitespace { " " } else { "" }.into(),
            byte_start: number(token.start.0)?,
            byte_end: number(token.end.0)?,
            code_point_start: number(token.idx.0)?,
            code_point_end: number(token.idx.0 + points)?,
            utf16_start: number(utf16_cursor)?,
            utf16_end: number(utf16_cursor + units)?,
            norm: token.norm.clone(),
            tag: token.tag.clone(),
            pos: token.pos.clone(),
            morphology: token.morphology.clone(),
            lemma: token.lemma.clone(),
            head: token.head.map(|index| number(index.0)).transpose()?,
            dep: token.dep.clone(),
            sentence_start: token.sentence_start,
            entity_iob: token.entity_iob.clone(),
            entity_type: token.entity_type.clone(),
            entity_id: token.entity_id.clone().unwrap_or_default(),
        });
        utf16_cursor += units;
        byte_cursor = token.end.0;
    }
    Ok(Document {
        text: doc.text().to_owned(),
        tokens,
        entities: spans(doc.entities())?,
        sentences: spans(doc.sentences())?,
        noun_chunks: spans(doc.noun_chunks())?,
    })
}

fn spans(spans: Option<&[spars::Span]>) -> Result<Option<Vec<Span>>> {
    spans
        .map(|spans| {
            spans
                .iter()
                .map(|span| {
                    Ok(Span {
                        start: number(span.start.0)?,
                        end: number(span.end.0)?,
                        label: span.label.clone(),
                        id: span.id.clone().unwrap_or_default(),
                    })
                })
                .collect()
        })
        .transpose()
}
