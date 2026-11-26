//! Finds the differing parts inside a pair of changed lines, at word granularity. Offsets are in chars.

use std::collections::HashMap;

use super::TextDiffOptions;
use super::myers;
use crate::compare::upper_char;

/// A range of chars inside a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextRange {
    pub start: usize,
    pub length: usize,
}

impl TextRange {
    pub fn new(start: usize, length: usize) -> Self {
        Self { start, length }
    }

    pub fn end(&self) -> usize {
        self.start + self.length
    }
}

pub fn compare_inline(
    left: &str,
    right: &str,
    options: TextDiffOptions,
) -> (Vec<TextRange>, Vec<TextRange>) {
    let l: Vec<char> = left.chars().collect();
    let r: Vec<char> = right.chars().collect();
    let lt = tokenize_chars(&l);
    let rt = tokenize_chars(&r);
    let mut keys = HashMap::new();
    let a: Vec<u32> = lt.iter().map(|t| key(&l, *t, options, &mut keys)).collect();
    let b: Vec<u32> = rt.iter().map(|t| key(&r, *t, options, &mut keys)).collect();
    let (mod_a, mod_b) = myers::compute(&a, &b);
    (
        ranges(&lt, &mod_a, &l, options),
        ranges(&rt, &mod_b, &r, options),
    )
}

fn key(s: &[char], t: TextRange, options: TextDiffOptions, keys: &mut HashMap<String, u32>) -> u32 {
    let mut text: String = s[t.start..t.end()].iter().collect();
    if options.ignore_whitespace && s[t.start].is_whitespace() {
        text = " ".to_string();
    }
    if options.ignore_case {
        text = text.chars().map(upper_char).collect();
    }
    let next = keys.len() as u32;
    *keys.entry(text).or_insert(next)
}

/// Merges adjacent modified tokens into ranges. Whitespace-only ranges are dropped when whitespace is ignored.
fn ranges(
    tokens: &[TextRange],
    modified: &[bool],
    s: &[char],
    options: TextDiffOptions,
) -> Vec<TextRange> {
    let mut result: Vec<TextRange> = Vec::new();
    for (t, &m) in tokens.iter().zip(modified) {
        if !m || (options.ignore_whitespace && s[t.start].is_whitespace()) {
            continue;
        }
        match result.last_mut() {
            Some(last) if last.end() == t.start => last.length += t.length,
            _ => result.push(*t),
        }
    }
    result
}

/// Tokens are runs of letters/digits/underscore, runs of whitespace, or single other characters.
pub fn tokenize(s: &str) -> Vec<TextRange> {
    tokenize_chars(&s.chars().collect::<Vec<_>>())
}

fn tokenize_chars(s: &[char]) -> Vec<TextRange> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut tokens = Vec::new();
    let mut n = 0;
    while n < s.len() {
        let start = n;
        let c = s[n];
        if is_word(c) {
            while n < s.len() && is_word(s[n]) {
                n += 1;
            }
        } else if c.is_whitespace() {
            while n < s.len() && s[n].is_whitespace() {
                n += 1;
            }
        } else {
            n += 1;
        }
        tokens.push(TextRange::new(start, n - start));
    }
    tokens
}
