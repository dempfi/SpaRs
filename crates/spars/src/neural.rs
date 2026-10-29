use crate::config::*;
use crate::{Doc, Error, Model, Result, TokenIndex};
mod matrix;
use matrix::{product_transposed, Dense};
pub(crate) type Matrix = Vec<Vec<f32>>;
fn linear_into(model: &Model, p: &Linear, x: &[f32], output: &mut [f32]) {
    let w = model.tensor(&p.w);
    let b = model.tensor(&p.b);
    for ((value, row), bias) in output
        .iter_mut()
        .zip(w.data.chunks_exact(x.len()))
        .zip(&b.data)
    {
        *value = row.iter().zip(x).map(|(a, b)| a * b).sum::<f32>() + bias;
    }
}
fn affine(model: &Model, p: &Linear, input: &Dense) -> Dense {
    let w = model.tensor(&p.w);
    let b = model.tensor(&p.b);
    let mut output = product_transposed(input, &w.data, b.data.len());
    for row in output.data.chunks_exact_mut(output.cols) {
        for (value, bias) in row.iter_mut().zip(&b.data) {
            *value += bias;
        }
    }
    output
}
pub(crate) fn linear_rows(model: &Model, p: &Linear, input: &Matrix) -> Matrix {
    if input.is_empty() {
        return Vec::new();
    }
    affine(model, p, &Dense::from_rows(input)).into_rows()
}
fn block(model: &Model, p: &Block, input: &Dense) -> Dense {
    let w = model.tensor(&p.maxout.w);
    let pieces = w.shape[1];
    let width = w.shape[0];
    let projected = affine(model, &p.maxout, input);
    let g = model.tensor(&p.norm.g);
    let b = model.tensor(&p.norm.b);
    let mut output = Dense::zeroed(input.rows, width);
    for i in 0..input.rows {
        let row = output.row_mut(i);
        for (value, candidates) in row.iter_mut().zip(projected.row(i).chunks_exact(pieces)) {
            *value = candidates.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        }
        let mean = row.iter().sum::<f32>() / width as f32;
        let variance =
            row.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / width as f32 + 1e-8;
        let inv = 1. / variance.sqrt();
        for (j, value) in row.iter_mut().enumerate() {
            *value = (*value - mean) * inv * g.data[j] + b.data[j];
        }
    }
    output
}
impl Model {
    /// Contextual features from the shared tok2vec network (one row per token).
    pub fn tok2vec(&self, doc: &Doc) -> Vec<Vec<f32>> {
        self.encode(doc, &self.config.tok2vec)
    }
    pub(crate) fn features(&self, doc: &Doc, i: usize, attrs: &[Feature]) -> Vec<u64> {
        let t = &doc.tokens[i];
        let word = doc.token_text(TokenIndex(i)).unwrap();
        let chars: Vec<_> = word.chars().collect();
        attrs
            .iter()
            .map(|a| match a {
                Feature::Norm => self.string_id(&t.norm),
                Feature::Prefix => self.string_id(&chars[..1].iter().collect::<String>()),
                Feature::Suffix => self.string_id(
                    &chars[chars.len().saturating_sub(3)..]
                        .iter()
                        .collect::<String>(),
                ),
                Feature::Shape => self.string_id(&self.shape(word)),
                Feature::Spacy => u64::from(t.whitespace),
                Feature::IsSpace => u64::from(word.chars().all(crate::tokenizer::is_space)),
            })
            .collect()
    }
    pub(crate) fn encode(&self, doc: &Doc, p: &Encoder) -> Matrix {
        self.encode_traced(doc, p, None)
    }
    pub(crate) fn encode_traced(
        &self,
        doc: &Doc,
        p: &Encoder,
        mut trace: Option<&mut Vec<Matrix>>,
    ) -> Matrix {
        let width = p.width;
        let n = doc.tokens.len();
        if n == 0 {
            return vec![];
        }
        let hashes = &p.hashes;
        let mut input = Dense::zeroed(
            n,
            width * (hashes.len() + usize::from(p.static_vectors.is_some())),
        );
        for i in 0..n {
            let ids = self.features(doc, i, &p.attrs);
            let row = input.row_mut(i);
            for (feature, (id, h)) in ids.iter().zip(hashes).enumerate() {
                let e = self.tensor(&h.params.e);
                let target = &mut row[feature * width..(feature + 1) * width];
                for key in crate::hash::keys(*id, h.seed) {
                    let at = (key as usize % e.shape[0]) * width;
                    for (value, embedding) in target.iter_mut().zip(&e.data[at..at + width]) {
                        *value += embedding;
                    }
                }
            }
        }
        if let Some(static_vectors) = &p.static_vectors {
            let projection = self.tensor(&static_vectors.w);
            let mut vectors = Dense::zeroed(n, projection.shape[1]);
            for i in 0..n {
                if let Some(vector) = self.vector(doc.token_text(TokenIndex(i)).unwrap()) {
                    vectors.row_mut(i).copy_from_slice(vector);
                }
            }
            let projected = product_transposed(&vectors, &projection.data, width);
            for i in 0..n {
                input.row_mut(i)[hashes.len() * width..].copy_from_slice(projected.row(i));
            }
        }
        let mixed = block(self, &p.mix, &input);
        if let Some(t) = trace.as_mut() {
            t.push((0..n).map(|i| mixed.row(i).to_vec()).collect());
        }
        // Padding evolves through residual layers, matching Thinc with_array.
        let pad = p.pad;
        let mut x = Dense::zeroed(n + pad * 2, width);
        x.data[pad * width..(pad + n) * width].copy_from_slice(&mixed.data);
        let mut context = Dense::zeroed(x.rows, 0);
        for (layer, window) in p.layers.iter().zip(&p.windows) {
            context.cols = width * (2 * window + 1);
            context.data.resize(context.rows * context.cols, 0.);
            context.data.fill(0.);
            for i in 0..x.rows {
                let target = context.row_mut(i);
                for slot in 0..2 * window + 1 {
                    if let Some(source) = (i + slot).checked_sub(*window).filter(|j| *j < x.rows) {
                        target[slot * width..(slot + 1) * width].copy_from_slice(x.row(source));
                    }
                }
            }
            let delta = block(self, layer, &context);
            for (value, change) in x.data.iter_mut().zip(delta.data) {
                *value += change;
            }
            if let Some(t) = trace.as_mut() {
                t.push((pad..pad + n).map(|i| x.row(i).to_vec()).collect());
            }
        }
        (pad..pad + n).map(|i| x.row(i).to_vec()).collect()
    }
}

pub(crate) struct Scorer<'a> {
    model: &'a Model,
    p: &'a Transition,
    cache: Dense,
    nf: usize,
    no: usize,
    np: usize,
    activation: Vec<f32>,
    hidden: Vec<f32>,
    output: Vec<f32>,
}
impl<'a> Scorer<'a> {
    pub fn new(model: &'a Model, p: &'a Transition, x: &Matrix) -> Self {
        let w = model.tensor(&p.lower.w);
        let nf = w.shape[0];
        let no = w.shape[1];
        let np = w.shape[2];
        let columns = nf * no * np;
        let mut cache = Dense::zeroed(x.len() + 1, columns);
        cache
            .row_mut(0)
            .copy_from_slice(&model.tensor(&p.lower.pad).data);
        if !x.is_empty() {
            let reduced = affine(model, &p.reduce, &Dense::from_rows(x));
            let projected = product_transposed(&reduced, &w.data, columns);
            cache.data[columns..].copy_from_slice(&projected.data);
        }
        Self {
            model,
            p,
            cache,
            nf,
            no,
            np,
            activation: vec![0.; no * np],
            hidden: vec![0.; no],
            output: vec![0.; p.actions.len()],
        }
    }
    pub fn scores(&mut self, ids: &[Option<usize>]) -> &[f32] {
        self.activation.fill(0.);
        for (f, id) in ids.iter().enumerate().take(self.nf) {
            let row = self.cache.row(id.map_or(0, |i| i + 1));
            for (j, v) in self.activation.iter_mut().enumerate() {
                *v += row[f * self.no * self.np + j]
            }
        }
        for (v, b) in self
            .activation
            .iter_mut()
            .zip(&self.model.tensor(&self.p.lower.b).data)
        {
            *v += b
        }
        for (value, pieces) in self
            .hidden
            .iter_mut()
            .zip(self.activation.chunks_exact(self.np))
        {
            *value = pieces.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        }
        linear_into(self.model, &self.p.upper, &self.hidden, &mut self.output);
        &self.output
    }
}
pub(crate) fn validate(m: &Model) -> Result<()> {
    let bad = |s: &str| Error::Model(s.into());
    let tensor = |key: &TensorRef| -> Result<&crate::model::Tensor> {
        m.tensors.get(&key.0).ok_or_else(|| bad("missing tensor"))
    };
    let shape = |key: &TensorRef, expected: &[usize]| -> Result<()> {
        if tensor(key)?.shape != expected {
            Err(bad(&format!(
                "shape mismatch {key:?}, expected {expected:?}"
            )))
        } else {
            Ok(())
        }
    };
    let vectors = m
        .tensors
        .get("vectors")
        .ok_or_else(|| bad("missing vectors"))?;
    if vectors.shape.len() != 2 {
        return Err(bad("vectors rank"));
    }
    if m.vector_keys
        .rows
        .iter()
        .any(|&r| r as usize >= vectors.shape[0])
    {
        return Err(bad("vector row out of bounds"));
    }
    let enc = |p: &Encoder| -> Result<usize> {
        let width = p.width;
        if width == 0 || width > 4096 {
            return Err(bad("invalid width"));
        }
        let attrs = &p.attrs;
        let hashes = &p.hashes;
        if attrs.len() != hashes.len() || attrs.is_empty() {
            return Err(bad("feature count mismatch"));
        }
        for h in hashes {
            let e = tensor(&h.params.e)?;
            if e.shape.len() != 2 || e.shape[1] != width {
                return Err(bad("embedding config"));
            }
        }
        if let Some(static_vectors) = &p.static_vectors {
            if vectors.shape[1] == 0 {
                return Err(bad("static projection requires vectors"));
            }
            shape(&static_vectors.w, &[width, vectors.shape[1]])?;
        }
        let block = |b: &Block, input: usize| -> Result<()> {
            let w = tensor(&b.maxout.w)?;
            if w.shape.len() != 3 || w.shape[0] != width || w.shape[2] != input {
                return Err(bad("maxout shape"));
            }
            shape(&b.maxout.b, &[width, w.shape[1]])?;
            shape(&b.norm.g, &[width])?;
            shape(&b.norm.b, &[width])
        };
        block(
            &p.mix,
            width * (attrs.len() + usize::from(p.static_vectors.is_some())),
        )?;
        let layers = &p.layers;
        let windows = &p.windows;
        if layers.len() != windows.len() {
            return Err(bad("window count"));
        }
        let mut total = 0;
        for (b, w) in layers.iter().zip(windows) {
            let w = *w;
            if w > 16 {
                return Err(bad("invalid window"));
            }
            total += w;
            block(b, width * (w * 2 + 1))?
        }
        if p.pad != total {
            return Err(bad("encoder padding"));
        }
        Ok(width)
    };
    let width = enc(&m.config.tok2vec)?;
    let nerwidth = enc(&m.config.ner.tok2vec)?;
    let tag = &m.config.tagger;
    let labels = &tag.labels;
    if labels.is_empty() {
        return Err(bad("invalid labels"));
    }
    shape(&tag.params.w, &[labels.len(), width])?;
    shape(&tag.params.b, &[labels.len()])?;
    for (p, nf, input) in [
        (&m.config.parser, 8, width),
        (&m.config.ner.transition, 3, nerwidth),
    ] {
        let low = tensor(&p.lower.w)?;
        if low.shape.len() != 4 || low.shape[0] != nf {
            return Err(bad("transition features"));
        }
        let no = low.shape[1];
        let np = low.shape[2];
        if np < 2 {
            return Err(Error::Unsupported(
                "transition activation without maxout".into(),
            ));
        }
        let ni = low.shape[3];
        shape(&p.lower.b, &[no, np])?;
        shape(&p.lower.pad, &[1, nf, no, np])?;
        shape(&p.reduce.w, &[ni, input])?;
        shape(&p.reduce.b, &[ni])?;
        let actions = &p.actions;
        if actions.is_empty() {
            return Err(bad("invalid actions"));
        }
        shape(&p.upper.w, &[actions.len(), no])?;
        shape(&p.upper.b, &[actions.len()])?;
    }
    let expected = vec![
        Component::Tok2vec,
        Component::Tagger,
        Component::Parser,
        Component::AttributeRuler,
        Component::Lemmatizer,
        Component::Ner,
    ];
    if m.config.format_version == 1 && m.config.pipeline != expected {
        return Err(Error::Unsupported("pipeline order".into()));
    }
    crate::pipeline::validate_order(&m.config.pipeline)?;
    Ok(())
}

#[derive(serde::Serialize)]
pub(crate) struct TransitionTrace {
    pub ids: Vec<i64>,
    pub scores: Vec<f32>,
    pub valid: Vec<bool>,
    pub action: usize,
}
