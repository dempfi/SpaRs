//! Reusable dependency matching over immutable native documents.
use crate::{
    document::NativeDocument,
    errors,
    matcher_types::{invalid, TokenConstraint},
};
use napi::{
    bindgen_prelude::{AsyncTask, Utf16String},
    Env, Result, Task,
};
use napi_derive::napi;
use std::sync::Arc;

/// Connects a pattern node to a node declared earlier in the same pattern.
/// A match requires `relation` to hold from the `left` node's token to this node's token.
#[napi(object)]
pub struct DependencyLink {
    /// The `id` of an earlier node in `DependencyPattern.nodes`. `add` throws
    /// `SPARS_INVALID_PATTERN` if no earlier node has this `id`.
    pub left: Utf16String,
    /// A spaCy `DependencyMatcher` operator (spaCy's `REL_OP`), read as "left RELATION this".
    /// For example, `>` means this token is a direct child of `left`, `<` means it is the head of
    /// `left`, and `.` means it immediately follows `left`. Order operators (`.`, `.*`, `;`, `;*`)
    /// only relate tokens in the same dependency tree; stored sentence annotations are not used.
    #[napi(ts_type = "DependencyRelation")]
    pub relation: Utf16String,
}

/// One token to find, with the conditions it must satisfy and its relation to an earlier node.
#[napi(object)]
pub struct DependencyNode {
    /// Unique within the pattern; later nodes refer to it in `link.left`.
    pub id: Utf16String,
    /// All must hold for the same token; empty matches any token.
    pub constraints: Vec<TokenConstraint>,
    /// Omit on the first node; required on every later node.
    pub link: Option<DependencyLink>,
}

/// Nodes in declaration order; each match reports its tokens in this order.
#[napi(object)]
pub struct DependencyPattern {
    pub nodes: Vec<DependencyNode>,
}

/// A match of `rule`; `tokens[i]` is the token matched by pattern node `i`.
#[napi(object, object_from_js = false)]
pub struct DependencyMatch {
    pub rule: String,
    /// Token indices in pattern node order.
    #[napi(ts_type = "Array<import('./units.js').TokenIndex>")]
    pub tokens: Vec<u32>,
}

fn relation(value: &str) -> errors::Result<spars::Relation> {
    use spars::Relation::*;
    Ok(match value {
        "<" => Parent,
        ">" => Child,
        "<<" => Ancestor,
        ">>" => Descendant,
        "." => ImmediatelyPrecedes,
        ".*" => Precedes,
        ";" => ImmediatelyFollows,
        ";*" => Follows,
        "$+" => ImmediateRightSibling,
        "$-" => ImmediateLeftSibling,
        "$++" => RightSibling,
        "$--" => LeftSibling,
        ">+" => ImmediateRightChild,
        ">-" => ImmediateLeftChild,
        ">++" => RightChild,
        ">--" => LeftChild,
        "<+" => ImmediateRightParent,
        "<-" => ImmediateLeftParent,
        "<++" => RightParent,
        "<--" => LeftParent,
        _ => return Err(invalid("unknown dependency relation")),
    })
}

fn relation_name(value: spars::Relation) -> &'static str {
    use spars::Relation::*;
    match value {
        Parent => "<",
        Child => ">",
        Ancestor => "<<",
        Descendant => ">>",
        ImmediatelyPrecedes => ".",
        Precedes => ".*",
        ImmediatelyFollows => ";",
        Follows => ";*",
        ImmediateRightSibling => "$+",
        ImmediateLeftSibling => "$-",
        RightSibling => "$++",
        LeftSibling => "$--",
        ImmediateRightChild => ">+",
        ImmediateLeftChild => ">-",
        RightChild => ">++",
        LeftChild => ">--",
        ImmediateRightParent => "<+",
        ImmediateLeftParent => "<-",
        RightParent => "<++",
        LeftParent => "<--",
    }
}

impl DependencyPattern {
    fn into_native(self) -> errors::Result<spars::DependencyPattern> {
        let nodes = self
            .nodes
            .into_iter()
            .map(|node| {
                Ok(spars::DependencyNode {
                    id: errors::text(&node.id)?,
                    constraints: node
                        .constraints
                        .into_iter()
                        .map(TokenConstraint::into_native)
                        .collect::<errors::Result<Vec<_>>>()?,
                    link: node
                        .link
                        .map(|link| -> errors::Result<spars::DependencyLink> {
                            Ok(spars::DependencyLink {
                                left: errors::text(&link.left)?,
                                relation: relation(&errors::text(&link.relation)?)?,
                            })
                        })
                        .transpose()?,
                })
            })
            .collect::<errors::Result<Vec<_>>>()?;
        Ok(spars::DependencyPattern { nodes })
    }

    fn from_native(pattern: &spars::DependencyPattern) -> Self {
        Self {
            nodes: pattern
                .nodes
                .iter()
                .map(|node| DependencyNode {
                    id: node.id.clone().into(),
                    constraints: node
                        .constraints
                        .iter()
                        .map(TokenConstraint::from_native)
                        .collect(),
                    link: node.link.as_ref().map(|link| DependencyLink {
                        left: link.left.clone().into(),
                        relation: relation_name(link.relation).to_owned().into(),
                    }),
                })
                .collect(),
        }
    }
}

#[derive(Default)]
/// Finds token groups linked by dependency relations, like spaCy's `DependencyMatcher`.
#[napi]
pub struct DependencyMatcher {
    inner: Arc<spars::DependencyMatcher>,
}

#[napi]
impl DependencyMatcher {
    /// Pass a model to enable lexical flag conditions with its language rules.
    #[napi(constructor)]
    pub fn new(model: Option<&crate::Model>) -> Self {
        let matcher = match model {
            Some(model) => spars::DependencyMatcher::with_lexicon(model.inner.lexicon()),
            None => spars::DependencyMatcher::new(),
        };
        Self {
            inner: Arc::new(matcher),
        }
    }

    /// Number of rules, like spaCy `len(matcher)`.
    #[napi(getter)]
    pub fn size(&self, env: Env) -> Result<u32> {
        errors::number(self.inner.len()).map_err(|error| error.into_napi(env))
    }

    #[napi(strict)]
    pub fn contains(&self, env: Env, rule: Utf16String) -> Result<bool> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        Ok(self.inner.contains(&rule))
    }

    /// Patterns of `rule` in addition order, or null when `rule` is not registered.
    #[napi(strict)]
    pub fn get(&self, env: Env, rule: Utf16String) -> Result<Option<Vec<DependencyPattern>>> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        Ok(self.inner.get(&rule).map(|patterns| {
            patterns
                .iter()
                .map(DependencyPattern::from_native)
                .collect()
        }))
    }

    /// Append patterns to `rule`; all are validated before the matcher changes. Throws
    /// `SPARS_INVALID_PATTERN` for an empty pattern, a duplicate `id`, a link on the first node, a
    /// missing link on a later node, a `left` that is not an earlier node, or flag conditions
    /// without a model, and `SPARS_BUSY` while `findMatches` is pending.
    #[napi(strict)]
    pub fn add(
        &mut self,
        env: Env,
        rule: Utf16String,
        patterns: Vec<DependencyPattern>,
    ) -> Result<()> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        let matcher = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        let patterns = patterns
            .into_iter()
            .map(DependencyPattern::into_native)
            .collect::<errors::Result<Vec<_>>>()
            .map_err(|error| error.into_napi(env))?;
        matcher
            .add(rule, patterns)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Throws `SPARS_INVALID_PATTERN` for an unregistered rule and `SPARS_BUSY` while `findMatches` is pending.
    #[napi(strict)]
    pub fn remove(&mut self, env: Env, rule: Utf16String) -> Result<()> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        let matcher = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        matcher
            .remove(&rule)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Matches on Node's worker pool, grouped by rule in registration order, then by pattern in
    /// addition order. A matcher with rules needs a parsed document: missing, out-of-range or cyclic
    /// heads reject with `SPARS_INFERENCE`. Conditions fail as in `TokenMatcher.findMatches`:
    /// `SPARS_INFERENCE` when an `equals` condition on `lemma`, `pos`, `tag`, `dep` or `morphology`
    /// meets a nonempty document in which no token has that annotation (`''` counts as missing
    /// except for `morphology`), like spaCy's error E155, while tokens without it compare as `''`;
    /// `SPARS_UNSUPPORTED` when a condition compares noncanonical `morphology`. Rejects with
    /// `SPARS_BUSY` or `SPARS_INPUT_LIMIT` under `configureExecution` and `configureInputLimits`.
    #[napi(strict, ts_return_type = "Promise<Array<DependencyMatch>>")]
    pub fn find_matches(&self, document: &NativeDocument) -> AsyncTask<DependencyMatchTask> {
        AsyncTask::new(DependencyMatchTask {
            matcher: Arc::clone(&self.inner),
            document: Arc::clone(&document.inner),
        })
    }
}

pub struct DependencyMatchTask {
    matcher: Arc<spars::DependencyMatcher>,
    document: Arc<spars::Doc>,
}

#[napi]
impl Task for DependencyMatchTask {
    type Output = errors::Result<Vec<DependencyMatch>>;
    type JsValue = Vec<DependencyMatch>;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self
            .matcher
            .find_matches(&self.document)
            .map_err(Into::into)
            .and_then(|matches| {
                matches
                    .into_iter()
                    .map(|matched| {
                        Ok(DependencyMatch {
                            rule: matched.rule,
                            tokens: matched
                                .tokens
                                .into_iter()
                                .map(|token| errors::number(token.0))
                                .collect::<errors::Result<Vec<_>>>()?,
                        })
                    })
                    .collect()
            }))
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
        output.map_err(|error| error.into_napi(env))
    }
}
