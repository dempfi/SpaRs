use crate::config::{Exception, TokenizerConfig};
use crate::{ByteOffset, CodePointOffset, Doc, Model, Result, Token};
use fancy_regex::Regex;
use std::collections::HashMap;
pub(crate) struct Tokenizer {
    prefix: Regex,
    suffix: Regex,
    infix: Regex,
    url: std::sync::Arc<Regex>,
    rules: HashMap<String, Vec<Exception>>,
    matcher_patterns: HashMap<String, Vec<Vec<String>>>,
}
impl Tokenizer {
    /// The compiled URL pattern, shared with the model's lexicon for `LIKE_URL`.
    pub(crate) fn url(&self) -> std::sync::Arc<Regex> {
        std::sync::Arc::clone(&self.url)
    }
    pub fn new(v: &TokenizerConfig) -> Result<Self> {
        if v.token_match.is_some() {
            return Err(crate::Error::Unsupported("token_match".into()));
        }
        if v.faster_heuristics == Some(false) {
            return Err(crate::Error::Unsupported(
                "tokenizer faster_heuristics must be true".into(),
            ));
        }
        let prefix = Regex::new(&v.prefix)?;
        let suffix = Regex::new(&v.suffix)?;
        let infix = Regex::new(&v.infix)?;
        let rules = v.rules.clone();
        let mut tokenizer = Self {
            prefix,
            suffix,
            infix,
            matcher_patterns: HashMap::new(),
            url: std::sync::Arc::new(Regex::new(&v.url)?),
            rules,
        };
        // spaCy builds phrase patterns with exception handling disabled.
        // Matching the same text with different token boundaries is insufficient.
        for word in tokenizer.rules.keys() {
            let prefix_length = tokenizer
                .prefix
                .find(word)?
                .map_or(0, |m| m.end() - m.start());
            let suffix_length = tokenizer
                .suffix
                .find(word)?
                .map_or(0, |m| m.end() - m.start());
            if word.contains(' ')
                || prefix_length > 0
                || suffix_length > 0
                || tokenizer.infix.is_match(word)?
            {
                let pattern: Vec<String> = tokenizer
                    .split_whitespace(word, false)?
                    .iter()
                    .map(|(a, b, _)| word[*a..*b].to_owned())
                    .collect();
                if let Some(first) = pattern.first() {
                    tokenizer
                        .matcher_patterns
                        .entry(first.clone())
                        .or_default()
                        .push(pattern);
                }
            }
        }
        Ok(tokenizer)
    }
    fn split_whitespace(
        &self,
        text: &str,
        with_special_cases: bool,
    ) -> Result<Vec<(usize, usize, Option<String>)>> {
        let mut raw = vec![];
        let mut start = 0;
        let mut in_ws = text.chars().next().is_some_and(is_space);
        for (i, c) in text.char_indices() {
            if is_space(c) != in_ws {
                if start < i {
                    self.split(&text[start..i], start, &mut raw, with_special_cases)?;
                }
                start = if c == ' ' { i + 1 } else { i };
                in_ws = !in_ws;
            }
        }
        if start < text.len() {
            self.split(&text[start..], start, &mut raw, with_special_cases)?;
        }
        Ok(raw)
    }
    fn split(
        &self,
        s: &str,
        offset: usize,
        out: &mut Vec<(usize, usize, Option<String>)>,
        with_special_cases: bool,
    ) -> Result<()> {
        let mut start = 0;
        let mut end = s.len();
        let mut suffixes = vec![];
        while start < end && !(with_special_cases && self.rules.contains_key(&s[start..end])) {
            let text = &s[start..end];
            let p = self.prefix.find(text)?.map_or(0, |m| m.end() - m.start());
            if p > 0 && with_special_cases && self.rules.contains_key(&text[p..]) {
                out.push((offset + start, offset + start + p, None));
                start += p;
                break;
            }
            let z = self
                .suffix
                .find(&text[p..])?
                .map_or(0, |m| m.end() - m.start());
            if z > 0 && with_special_cases && self.rules.contains_key(&text[..text.len() - z]) {
                suffixes.push((offset + end - z, offset + end, None));
                end -= z;
                break;
            }
            if p == 0 && z == 0 {
                break;
            }
            if p > 0 {
                out.push((offset + start, offset + start + p, None));
                start += p
            }
            if z > 0 {
                suffixes.push((offset + end - z, offset + end, None));
                end -= z
            }
        }
        if start < end {
            let text = &s[start..end];
            if let Some(rule) = self.rules.get(text).filter(|_| with_special_cases) {
                let mut at = offset + start;
                for t in rule {
                    let len = t.orth.as_str().len();
                    out.push((at, at + len, t.norm.clone()));
                    at += len;
                }
            } else if self.url.is_match(text)? {
                out.push((offset + start, offset + end, None))
            } else {
                let mut at = 0;
                for m in self.infix.find_iter(text) {
                    let m = m?;
                    if m.start() == 0 {
                        continue;
                    }
                    if m.start() != at {
                        out.push((offset + start + at, offset + start + m.start(), None))
                    }
                    if m.end() != m.start() {
                        out.push((offset + start + m.start(), offset + start + m.end(), None))
                    }
                    at = m.end();
                }
                if at < text.len() {
                    out.push((offset + start + at, offset + end, None))
                }
            }
        }
        out.extend(suffixes.into_iter().rev());
        Ok(())
    }
}
pub(crate) fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}
impl Model {
    pub fn tokenize(&self, text: &str) -> Result<Doc> {
        let raw = self.tokenizer.split_whitespace(text, true)?;
        let mut matches = vec![];
        for (i, (a, b, _)) in raw.iter().enumerate() {
            if let Some(patterns) = self.tokenizer.matcher_patterns.get(&text[*a..*b]) {
                for pattern in patterns {
                    let j = i + pattern.len();
                    if j <= raw.len()
                        && raw[i..j]
                            .iter()
                            .zip(pattern)
                            .all(|((a, b, _), word)| &text[*a..*b] == word)
                    {
                        matches.push((
                            pattern.len(),
                            i,
                            j,
                            self.tokenizer.rules.get(&text[*a..raw[j - 1].1]),
                        ));
                    }
                }
            }
        }
        matches.sort_by_key(|(len, i, _, _)| (std::cmp::Reverse(*len), *i));
        let mut used = vec![false; raw.len()];
        let mut selected = HashMap::new();
        for (_, i, j, r) in matches {
            if !used[i] && !used[j - 1] {
                if let Some(rule) = r {
                    selected.insert(i, (j, rule));
                }
            }
            // Upstream also reserves tokens from rejected overlapping matches.
            used[i..j].fill(true);
        }
        let mut replaced = vec![];
        let mut i = 0;
        while i < raw.len() {
            if let Some((j, r)) = selected.get(&i) {
                let mut a = raw[i].0;
                for t in *r {
                    let word = t.orth.as_str();
                    replaced.push((a, a + word.len(), t.norm.clone()));
                    a += word.len();
                    if text.as_bytes().get(a) == Some(&b' ') && a < raw[*j - 1].1 {
                        a += 1
                    }
                }
                i = *j;
            } else {
                replaced.push(raw[i].clone());
                i += 1
            }
        }
        let mut tokens = vec![];
        let mut cp = 0;
        let mut prev = 0;
        for (a, b, norm) in replaced {
            cp += text[prev..a].chars().count();
            let mut t = Token::new(
                ByteOffset(a),
                ByteOffset(b),
                CodePointOffset(cp),
                norm.unwrap_or_else(|| self.norm(&text[a..b])),
            );
            t.whitespace = text.as_bytes().get(b) == Some(&b' ');
            tokens.push(t);
            prev = a;
        }
        Ok(Doc {
            text: text.into(),
            tokens,
            entities: None,
            sentences: None,
            noun_chunks: None,
            tensor: Vec::new(),
            dependency_index: Default::default(),
        })
    }
}
#[cfg(test)]
impl Tokenizer {
    pub(crate) fn regex_spans(&self, text: &str, kind: &str) -> Result<Vec<[usize; 2]>> {
        let re = match kind {
            "prefix" => &self.prefix,
            "suffix" => &self.suffix,
            "infix" => &self.infix,
            "url" => &self.url,
            _ => unreachable!(),
        };
        let convert = |m: fancy_regex::Match<'_>| {
            [
                text[..m.start()].chars().count(),
                text[..m.end()].chars().count(),
            ]
        };
        if kind == "infix" {
            Ok(re
                .find_iter(text)
                .map(|m| m.map(convert).map_err(crate::Error::from))
                .collect::<std::result::Result<_, _>>()?)
        } else {
            Ok(re.find(text)?.map(convert).into_iter().collect())
        }
    }
}
