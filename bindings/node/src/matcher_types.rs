//! Concrete shared matcher inputs; external strings are validated before compilation.
use crate::errors;
use napi::bindgen_prelude::{Either, Either3, Utf16String};
use napi_derive::napi;

#[napi(object)]
pub struct TokenPredicate {
    pub kind: Utf16String,
    // scripts/declarations.mts replaces this interface with the precise TokenConstraint union.
    // `==`, `!=`, `>=`, `<=`, `>` or `<` for `compare`.
    pub operator: Option<Utf16String>,
    // A string for `equals`; a boolean for `flag`; a finite number for `compare`.
    pub value: Option<Either3<Utf16String, bool, f64>>,
    // Strings for `in`, `not_in`, `is_subset`, `is_superset`, `intersects` and the
    // morphology predicates; safe integers for the `*_integers` predicates.
    pub values: Option<Either<Vec<Utf16String>, Vec<f64>>>,
}

#[napi(object)]
pub struct TokenConstraint {
    pub attribute: Utf16String,
    pub predicate: TokenPredicate,
}

pub(crate) fn invalid(message: &str) -> errors::BindingError {
    spars::Error::Pattern(message.into()).into()
}

// JavaScript numbers are doubles; accept only exact integers that round-trip.
fn integer(value: f64) -> errors::Result<i64> {
    const SAFE: f64 = 9_007_199_254_740_991.0;
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > SAFE {
        return Err(invalid("integer predicates require safe integers"));
    }
    Ok(value as i64)
}

fn comparison(operator: &str) -> errors::Result<spars::Comparison> {
    Ok(match operator {
        "==" => spars::Comparison::Equal,
        "!=" => spars::Comparison::NotEqual,
        ">=" => spars::Comparison::GreaterOrEqual,
        "<=" => spars::Comparison::LessOrEqual,
        ">" => spars::Comparison::Greater,
        "<" => spars::Comparison::Less,
        _ => return Err(invalid("unknown comparison operator")),
    })
}

fn operator_text(operator: spars::Comparison) -> &'static str {
    match operator {
        spars::Comparison::Equal => "==",
        spars::Comparison::NotEqual => "!=",
        spars::Comparison::GreaterOrEqual => ">=",
        spars::Comparison::LessOrEqual => "<=",
        spars::Comparison::Greater => ">",
        spars::Comparison::Less => "<",
    }
}

impl TokenConstraint {
    pub(crate) fn into_native(self) -> errors::Result<spars::TokenConstraint> {
        let attribute = match errors::text(&self.attribute)?.as_str() {
            "text" => spars::TokenAttribute::Text,
            "lower" => spars::TokenAttribute::Lower,
            "norm" => spars::TokenAttribute::Norm,
            "lemma" => spars::TokenAttribute::Lemma,
            "pos" => spars::TokenAttribute::Pos,
            "tag" => spars::TokenAttribute::Tag,
            "dep" => spars::TokenAttribute::Dep,
            "morphology" => spars::TokenAttribute::Morphology,
            "is_alpha" => spars::TokenAttribute::IsAlpha,
            "is_digit" => spars::TokenAttribute::IsDigit,
            "is_space" => spars::TokenAttribute::IsSpace,
            "is_punct" => spars::TokenAttribute::IsPunct,
            "like_num" => spars::TokenAttribute::LikeNum,
            "length" => spars::TokenAttribute::Length,
            "is_lower" => spars::TokenAttribute::IsLower,
            "is_upper" => spars::TokenAttribute::IsUpper,
            "is_title" => spars::TokenAttribute::IsTitle,
            "is_ascii" => spars::TokenAttribute::IsAscii,
            "is_currency" => spars::TokenAttribute::IsCurrency,
            "is_stop" => spars::TokenAttribute::IsStop,
            "is_bracket" => spars::TokenAttribute::IsBracket,
            "is_quote" => spars::TokenAttribute::IsQuote,
            "is_left_punct" => spars::TokenAttribute::IsLeftPunct,
            "is_right_punct" => spars::TokenAttribute::IsRightPunct,
            "like_url" => spars::TokenAttribute::LikeUrl,
            "like_email" => spars::TokenAttribute::LikeEmail,
            _ => return Err(invalid("unknown token attribute")),
        };
        let predicate = self.predicate;
        let kind = errors::text(&predicate.kind)?;
        if kind != "compare" && predicate.operator.is_some() {
            return Err(invalid("only compare accepts operator"));
        }
        let predicate = match kind.as_str() {
            "equals" | "flag" | "compare" => {
                if predicate.values.is_some() {
                    return Err(invalid(
                        "equals, flag and compare require value and forbid values",
                    ));
                }
                match (kind.as_str(), predicate.value) {
                    ("equals", Some(Either3::A(value))) => spars::Predicate::Equals {
                        value: errors::text(&value)?,
                    },
                    ("flag", Some(Either3::B(value))) => spars::Predicate::Flag { value },
                    ("compare", Some(Either3::C(value))) => spars::Predicate::Compare {
                        operator: comparison(&errors::text(
                            &predicate
                                .operator
                                .ok_or_else(|| invalid("compare requires operator"))?,
                        )?)?,
                        value: spars::FiniteNumber::new(value)
                            .ok_or_else(|| invalid("compare requires a finite number"))?,
                    },
                    ("equals", _) => return Err(invalid("equals requires a string value")),
                    ("flag", _) => return Err(invalid("flag requires a boolean value")),
                    _ => return Err(invalid("compare requires a number value")),
                }
            }
            "in_integers"
            | "not_in_integers"
            | "is_subset_integers"
            | "is_superset_integers"
            | "intersects_integers" => {
                if predicate.value.is_some() {
                    return Err(invalid("set predicates require values and forbid value"));
                }
                let values = match predicate.values {
                    Some(Either::B(values)) => values
                        .into_iter()
                        .map(integer)
                        .collect::<errors::Result<Vec<_>>>()?,
                    // An empty array is indistinguishable from an empty string list.
                    Some(Either::A(values)) if values.is_empty() => Vec::new(),
                    Some(Either::A(_)) => {
                        return Err(invalid("integer predicates require number values"))
                    }
                    None => return Err(invalid("set predicates require values")),
                };
                match kind.as_str() {
                    "in_integers" => spars::Predicate::InIntegers { values },
                    "not_in_integers" => spars::Predicate::NotInIntegers { values },
                    "is_subset_integers" => spars::Predicate::IsSubsetIntegers { values },
                    "is_superset_integers" => spars::Predicate::IsSupersetIntegers { values },
                    _ => spars::Predicate::IntersectsIntegers { values },
                }
            }
            _ => {
                if predicate.value.is_some() {
                    return Err(invalid("set predicates require values and forbid value"));
                }
                let values = match predicate.values {
                    Some(Either::A(values)) => values
                        .iter()
                        .map(|value| errors::text(value))
                        .collect::<errors::Result<Vec<_>>>()?,
                    Some(Either::B(values)) if values.is_empty() => Vec::new(),
                    Some(Either::B(_)) => {
                        return Err(invalid("string set predicates require string values"))
                    }
                    None => return Err(invalid("set predicates require values")),
                };
                match kind.as_str() {
                    "in" => spars::Predicate::In { values },
                    "not_in" => spars::Predicate::NotIn { values },
                    "morph_superset" => spars::Predicate::MorphSuperset { values },
                    "morph_intersects" => spars::Predicate::MorphIntersects { values },
                    "is_subset" => spars::Predicate::IsSubset { values },
                    "is_superset" => spars::Predicate::IsSuperset { values },
                    "intersects" => spars::Predicate::Intersects { values },
                    _ => return Err(invalid("unknown token predicate kind")),
                }
            }
        };
        Ok(spars::TokenConstraint {
            attribute,
            predicate,
        })
    }

    pub(crate) fn from_native(value: &spars::TokenConstraint) -> Self {
        let attribute = match value.attribute {
            spars::TokenAttribute::Text => "text",
            spars::TokenAttribute::Lower => "lower",
            spars::TokenAttribute::Norm => "norm",
            spars::TokenAttribute::Lemma => "lemma",
            spars::TokenAttribute::Pos => "pos",
            spars::TokenAttribute::Tag => "tag",
            spars::TokenAttribute::Dep => "dep",
            spars::TokenAttribute::Morphology => "morphology",
            spars::TokenAttribute::IsAlpha => "is_alpha",
            spars::TokenAttribute::IsDigit => "is_digit",
            spars::TokenAttribute::IsSpace => "is_space",
            spars::TokenAttribute::IsPunct => "is_punct",
            spars::TokenAttribute::LikeNum => "like_num",
            spars::TokenAttribute::Length => "length",
            spars::TokenAttribute::IsLower => "is_lower",
            spars::TokenAttribute::IsUpper => "is_upper",
            spars::TokenAttribute::IsTitle => "is_title",
            spars::TokenAttribute::IsAscii => "is_ascii",
            spars::TokenAttribute::IsCurrency => "is_currency",
            spars::TokenAttribute::IsStop => "is_stop",
            spars::TokenAttribute::IsBracket => "is_bracket",
            spars::TokenAttribute::IsQuote => "is_quote",
            spars::TokenAttribute::IsLeftPunct => "is_left_punct",
            spars::TokenAttribute::IsRightPunct => "is_right_punct",
            spars::TokenAttribute::LikeUrl => "like_url",
            spars::TokenAttribute::LikeEmail => "like_email",
        };
        let strings = |values: &Vec<String>| {
            Some(Either::A(
                values.iter().cloned().map(Into::into).collect::<Vec<_>>(),
            ))
        };
        // Accepted integers are safe integers, so the conversion is exact.
        let numbers = |values: &Vec<i64>| {
            Some(Either::B(
                values.iter().map(|value| *value as f64).collect::<Vec<_>>(),
            ))
        };
        let (kind, operator, value, values) = match &value.predicate {
            spars::Predicate::Equals { value } => {
                ("equals", None, Some(Either3::A(value.clone().into())), None)
            }
            spars::Predicate::Flag { value } => ("flag", None, Some(Either3::B(*value)), None),
            spars::Predicate::Compare { operator, value } => (
                "compare",
                Some(operator_text(*operator).to_owned().into()),
                Some(Either3::C(value.get())),
                None,
            ),
            spars::Predicate::In { values } => ("in", None, None, strings(values)),
            spars::Predicate::NotIn { values } => ("not_in", None, None, strings(values)),
            spars::Predicate::MorphSuperset { values } => {
                ("morph_superset", None, None, strings(values))
            }
            spars::Predicate::MorphIntersects { values } => {
                ("morph_intersects", None, None, strings(values))
            }
            spars::Predicate::IsSubset { values } => ("is_subset", None, None, strings(values)),
            spars::Predicate::IsSuperset { values } => ("is_superset", None, None, strings(values)),
            spars::Predicate::Intersects { values } => ("intersects", None, None, strings(values)),
            spars::Predicate::InIntegers { values } => ("in_integers", None, None, numbers(values)),
            spars::Predicate::IsSubsetIntegers { values } => {
                ("is_subset_integers", None, None, numbers(values))
            }
            spars::Predicate::IsSupersetIntegers { values } => {
                ("is_superset_integers", None, None, numbers(values))
            }
            spars::Predicate::IntersectsIntegers { values } => {
                ("intersects_integers", None, None, numbers(values))
            }
            spars::Predicate::NotInIntegers { values } => {
                ("not_in_integers", None, None, numbers(values))
            }
        };
        Self {
            attribute: attribute.to_owned().into(),
            predicate: TokenPredicate {
                kind: kind.to_owned().into(),
                operator,
                value,
                values,
            },
        }
    }
}
