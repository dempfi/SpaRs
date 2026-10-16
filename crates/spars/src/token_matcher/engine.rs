use super::start_index::StartIndex;
use super::{CompiledPattern, Quantifier, TokenMatch};
use crate::dependency_matcher::predicates::{validate_constraints, Needs, TokenValues};
use crate::{Doc, Lexicon, Result, TokenIndex};
use std::collections::HashSet;

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct State {
    pattern: usize,
    node: usize,
    start: usize,
}

/// Which patterns receive a new state at each token.
#[derive(Clone, Copy)]
pub(super) enum Starts<'a> {
    /// Only patterns whose first node can match the token, from the matcher's index.
    Indexed(&'a StartIndex),
    /// Every pattern with nodes, as spaCy does; the reference for the indexed path.
    #[cfg(test)]
    All,
}

pub(super) fn find(
    patterns: &[CompiledPattern],
    starts: Starts<'_>,
    doc: &Doc,
    lexicon: Option<&Lexicon>,
) -> Result<Vec<TokenMatch>> {
    if patterns.iter().all(|pattern| pattern.nodes.is_empty()) {
        return Ok(Vec::new());
    }
    let constraints = patterns
        .iter()
        .flat_map(|p| &p.items)
        .flat_map(|item| &item.constraints);
    let mut needs = Needs {
        lower: validate_constraints(constraints, doc)?,
        ..Needs::default()
    };
    for item in patterns.iter().flat_map(|pattern| &pattern.items) {
        needs.add(&item.lexical);
    }
    let values = TokenValues::new(doc, needs, lexicon)?;
    // For short patterns the extra lookup costs more than repeated suffix work.
    // Separate compiled paths keep ordinary matching free of per-state overhead.
    if patterns.iter().any(needs_suffix_cache) {
        run::<true>(patterns, starts, doc, &values)
    } else {
        run::<false>(patterns, starts, doc, &values)
    }
}

fn needs_suffix_cache(pattern: &CompiledPattern) -> bool {
    pattern
        .nodes
        .iter()
        .filter(|node| matches!(node.quantifier, Quantifier::Optional | Quantifier::Star))
        .take(9)
        .count()
        > 8
}

#[inline(never)]
fn run<const CACHE_SUFFIXES: bool>(
    patterns: &[CompiledPattern],
    starts: Starts<'_>,
    doc: &Doc,
    values: &TokenValues<'_>,
) -> Result<Vec<TokenMatch>> {
    let branching: Vec<bool> = if CACHE_SUFFIXES {
        patterns.iter().map(needs_suffix_cache).collect()
    } else {
        Vec::new()
    };
    let mut visited = HashSet::new();
    let mut states = Vec::new();
    let mut retained = Vec::new();
    let mut branches = Vec::new();
    let mut unique_states = HashSet::new();
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    let mut cache: Vec<Vec<Option<bool>>> =
        patterns.iter().map(|p| vec![None; p.items.len()]).collect();
    // Patterns with cached item results at the current token, so the reset at the next
    // token visits only them instead of every registered pattern.
    let mut touched: Vec<usize> = Vec::new();
    let mut dirty = vec![false; patterns.len()];
    let mut candidates = Vec::new();
    let mut emit = |pattern: usize, start: usize, end: usize| {
        if start < end && seen.insert((patterns[pattern].name.as_str(), start, end)) {
            output.push(TokenMatch {
                rule: patterns[pattern].name.clone(),
                start: TokenIndex(start),
                end: TokenIndex(end),
            });
        }
    };
    for index in 0..doc.tokens().len() {
        visited.clear();
        for pattern in touched.drain(..) {
            cache[pattern].fill(None);
            dirty[pattern] = false;
        }
        let token = doc.token(TokenIndex(index))?;
        // New states follow the states carried from earlier tokens, in pattern order.
        match starts {
            Starts::Indexed(start_index) => start_index.candidates(token, values, &mut candidates),
            #[cfg(test)]
            Starts::All => {
                // The reference also clears every pattern's cache, as before indexing.
                for values in &mut cache {
                    values.fill(None);
                }
                candidates.clear();
                candidates.extend(
                    (0..patterns.len()).filter(|&pattern| !patterns[pattern].nodes.is_empty()),
                );
            }
        }
        states.extend(candidates.iter().map(|&pattern| State {
            pattern,
            node: 0,
            start: index,
        }));
        for mut state in states.drain(..) {
            let pattern = &patterns[state.pattern];
            loop {
                // Reaching the same state at this token repeats the same emissions
                // and branches. Keep its first traversal, including optional skips.
                if CACHE_SUFFIXES && branching[state.pattern] && !visited.insert(state) {
                    break;
                }
                let node = &pattern.nodes[state.node];
                let matches = if let Some(value) = cache[state.pattern][node.item] {
                    value
                } else {
                    let item = &pattern.items[node.item];
                    let mut value =
                        item.lexical
                            .matches(values, index, &pattern.lengths, node.item)?;
                    if value {
                        for constraint in &item.constraints {
                            if !constraint.matches(token, values)? {
                                value = false;
                                break;
                            }
                        }
                    }
                    cache[state.pattern][node.item] = Some(value);
                    if !dirty[state.pattern] {
                        dirty[state.pattern] = true;
                        touched.push(state.pattern);
                    }
                    value
                };
                let final_node = state.node + 1 == pattern.nodes.len();
                match node.quantifier {
                    Quantifier::One | Quantifier::Negated => {
                        let matches = matches != matches!(node.quantifier, Quantifier::Negated);
                        if matches {
                            if final_node {
                                emit(state.pattern, state.start, index + 1);
                            } else {
                                state.node += 1;
                                retained.push(state);
                            }
                        }
                        break;
                    }
                    Quantifier::Star => {
                        if final_node {
                            emit(state.pattern, state.start, index);
                            if matches {
                                retained.push(state);
                            }
                            break;
                        }
                        if matches {
                            branches.push(state);
                        }
                        state.node += 1;
                    }
                    Quantifier::Optional => {
                        if final_node {
                            emit(state.pattern, state.start, index);
                            if matches {
                                emit(state.pattern, state.start, index + 1);
                            }
                            break;
                        }
                        state.node += 1;
                        if matches {
                            branches.push(state);
                        }
                    }
                }
            }
        }
        // Equivalent states have identical future emissions. Retaining their first
        // occurrence preserves match order while avoiding combinatorial path growth.
        unique_states.clear();
        for state in retained.drain(..).chain(branches.drain(..)) {
            if unique_states.insert(state) {
                states.push(state);
            }
        }
    }
    for state in states {
        if patterns[state.pattern].nodes[state.node..]
            .iter()
            .all(|node| matches!(node.quantifier, Quantifier::Star | Quantifier::Optional))
        {
            emit(state.pattern, state.start, doc.tokens().len());
        }
    }
    Ok(output)
}
