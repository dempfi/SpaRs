# Match grammatical relationships

`DependencyMatcher` finds tokens connected by dependency relationships. A pattern can identify a verb and its subject even when other words appear between them. Matches contain token indices in pattern-node order; they are not necessarily contiguous spans.

## Find verbs and subjects

Complete the model setup in the [developer guide](DEVELOPMENT.md). Register a pattern once and reuse the matcher across documents. Each node has an ID and token conditions. The first node has no link; each later node links from an earlier ID.

```rust
use spars::{DependencyMatcher, DependencyPattern, DependencyNode, DependencyLink,
    Predicate, Relation, TokenAttribute, TokenConstraint, Model, TokenIndex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = Model::load("assets/en_core_web_md-3.8.0")?;
    let mut matcher = DependencyMatcher::new();
    matcher.add("verb_subject", vec![DependencyPattern { nodes: vec![
        DependencyNode {
            id: "verb".into(), link: None,
            constraints: vec![TokenConstraint {
                attribute: TokenAttribute::Pos,
                predicate: Predicate::Equals { value: "VERB".into() },
            }],
        },
        DependencyNode {
            id: "subject".into(),
            link: Some(DependencyLink { left: "verb".into(), relation: Relation::Child }),
            constraints: vec![TokenConstraint {
                attribute: TokenAttribute::Dep,
                predicate: Predicate::Equals { value: "nsubj".into() },
            }],
        },
    ] }])?;
    let doc = model.process("Alice works in London.")?;
    let matches = matcher.find_matches(&doc)?;
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].rule, "verb_subject");
    assert_eq!(matches[0].tokens, [TokenIndex(1), TokenIndex(0)]);
    assert_eq!(doc.token(matches[0].tokens[1])?.text(), "Alice");
    Ok(())
}
```

All constraints on a node must match. Empty constraints match any token. Node IDs must be unique, and links must refer to earlier nodes. Registering invalid patterns returns an error without changing the matcher. Adding more patterns under an existing rule name appends them; `get`, `contains`, `remove`, `len`, and `is_empty` inspect or manage registered rules.

## Supported token conditions

`TokenAttribute` supports `Text`, `Lower`, `Norm`, `Lemma`, `Pos`, `Tag`, `Dep`, `Morphology`, the seventeen lexical flags (`IsAlpha`, `IsDigit`, `IsSpace`, `IsPunct`, `LikeNum`, `IsLower`, `IsUpper`, `IsTitle`, `IsAscii`, `IsCurrency`, `IsStop`, `IsBracket`, `IsQuote`, `IsLeftPunct`, `IsRightPunct`, `LikeUrl` and `LikeEmail`), and `Length`. `Predicate::Equals` compares an exact string; `In` and `NotIn` compare against a list of strings. Text comparisons are case-sensitive. `Lower` compares the token's pinned Unicode 15.0.0 lowercase text, like spaCy's `LOWER`; pattern values are compared as written and are not lowercased. See the [Token Matcher guide](TOKEN_MATCHER.md#match-without-regard-to-case) for its Unicode behavior. `IsSubset`, `IsSuperset` and `Intersects` compare a string attribute with a set of values, like spaCy's `IS_SUBSET`, `IS_SUPERSET` and `INTERSECTS`; see [Compare with a set of values](TOKEN_MATCHER.md#compare-with-a-set-of-values). `MorphSuperset` and `MorphIntersects` are equivalent forms of `IsSuperset` and `Intersects` that apply only to `Morphology`; other attributes, including `Lower`, return a pattern error. Lexical flags accept only `Predicate::Flag { value }` and need a matcher created with `DependencyMatcher::with_lexicon(model.lexicon())`; see the [Token Matcher guide](TOKEN_MATCHER.md#match-lexical-flags) for their rules. `Length` counts Unicode code points and accepts `Compare`, `InIntegers`, `NotInIntegers` and the integer set predicates `IsSubsetIntegers`, `IsSupersetIntegers` and `IntersectsIntegers`; see [Match by token length](TOKEN_MATCHER.md#match-by-token-length).

For morphology, `IsSuperset` (or `MorphSuperset`) requires every specified feature, `Intersects` (or `MorphIntersects`) requires at least one, and `IsSubset` requires every feature of the token to be listed. Use strings such as `Number=Sing` or `Tense=Past`. Multi-value features are tested individually: `Case=Nom` can match `Case=Acc,Nom`. A condition containing `Case=Acc,Nom` is a complete morphology value, not two separate set conditions; it never equals a single feature, so it never satisfies `IsSuperset` or `Intersects` and adds nothing to `IsSubset`. Use `In` to compare a complete morphology value. Morphology entries in `In`, `NotIn` and the set predicates must be well formed: an entry with an empty field or value, such as `Number=`, `=Sing` or `PronType=Int,`, or with spaces, returns a pattern error. spaCy accepts such entries and they never match, so a spaCy pattern containing one must drop it here. Morphology `In` and `NotIn` normalize feature order; direct `Equals` retains the supplied spelling, matching the pinned reference's distinction. Empty morphology is available data, distinct from a missing annotation. Exact morphology equality uses `"_"` for that known-empty value, while membership accepts either `""` or `"_"`. Missing morphology equals only `""`, so `In` never matches it and `NotIn` always does, as in spaCy. Repeated morphology fields use the last value; repeated values within one field remain significant. Restored documents must use canonical morphology strings when morphology is queried, as spaCy writes them: fields sorted as whole `Field=Value` strings (so `Case2=Acc` precedes `Case=Nom`), each field once, values sorted, and the `POS` field spelled `POS`, with a value already upper-case when it names a part of speech (`POS=NOUN`, while an unknown value such as `POS=foo` stays as written); malformed or noncanonical values return errors. Pattern entries are normalized the same way, and the `POS` alias is recognized with Unicode upper-casing, as in spaCy. spaCy's set predicates also depend on registration order: an entry spelled differently from its normalized form, such as `pos=noun`, registered before any document contains that analysis, makes spaCy store later documents' features in the pattern's spelling, so it never matches. SpaRs always compares normalized features, which is spaCy's result when the document is created first.

The matcher validates all dependency heads before matching; missing heads and cycles return errors instead of an empty match list. Token conditions check annotations as the [Token Matcher](TOKEN_MATCHER.md#missing-annotations) does, like spaCy, whose DependencyMatcher uses its token `Matcher`: on a nonempty document, an `Equals` condition on an annotation attribute needs at least one token with that annotation and returns `Error::MissingAnnotation` otherwise, and a token without the annotation compares as the empty string, with the special rules for missing morphology described there. An empty matcher returns no matches without requiring annotations. spaCy also keeps requiring an annotation compared by an `Equals` condition of a removed rule, even after every rule is removed; SpaRs checks only the current rules, an [accepted difference](SPACY_DIFFERENCES.md#annotations-required-by-removed-rules). Matching uses the document and, for lexical flag conditions, the lexicon the matcher was created with; it does not invoke model inference or download resources.

## Relationships

Relationships run from the linked earlier node to the new node. All 20 relationship operators from the pinned spaCy DependencyMatcher are supported:

| Rust relation | spaCy operator | New node |
|---|---|---|
| `Parent` / `Child` | `<` / `>` | Direct parent / child |
| `Ancestor` / `Descendant` | `<<` / `>>` | Any ancestor / descendant, excluding self |
| `ImmediatelyPrecedes` / `ImmediatelyFollows` | `.` / `;` | Next / previous token in the same dependency tree |
| `Precedes` / `Follows` | `.*` / `;*` | Any later / earlier token in the same dependency tree |
| `ImmediateRightSibling` / `ImmediateLeftSibling` | `$+` / `$-` | Sibling at the next / previous token position |
| `RightSibling` / `LeftSibling` | `$++` / `$--` | Any sibling to the right / left |
| `ImmediateRightChild` / `ImmediateLeftChild` | `>+` / `>-` | Child at the next / previous token position |
| `RightChild` / `LeftChild` | `>++` / `>--` | Any child to the right / left |
| `ImmediateRightParent` / `ImmediateLeftParent` | `<+` / `<-` | Parent at the next / previous token position |
| `RightParent` / `LeftParent` | `<++` / `<--` | Parent to the right / left |

Distinct node IDs can bind the same token when all relationships permit it. Duplicate patterns preserve duplicate matches. Results follow rule registration order, pattern order, dependency-tree groups in first-candidate order, and then token-index combinations in node order. This order is tested against official spaCy, including crossing dependencies.

## Scope and cost

The matcher compiles conditions once and uses per-call matching state. It can be shared for concurrent read-only matching. Broad patterns may have many valid combinations, so output size can still grow rapidly. Choose selective token conditions when the intended relationship allows them. Lexical flags are computed once per call for each token, and only for the flags that registered patterns use; spaCy instead stores them on each vocabulary entry, so repeated calls in SpaRs repeat this work.

This is a typed Rust API, not an importer for arbitrary spaCy matcher JSON. Regex, fuzzy matching, custom extensions, callbacks, span inputs, integer rule IDs, token-level attributes such as `IS_SENT_START`, `REGEX` on `Length`, and token repetition operators are not exposed yet. [Token Matcher](TOKEN_MATCHER.md) supports sequences and repetition; [PhraseMatcher](PHRASE_MATCHER.md) matches token phrases by text, lowercase, norm, annotations, lexical flags or length. Pattern JSON uses the Rust types' fields and rejects unknown fields or enum values. Full spaCy matcher compatibility is not claimed.
