//! A character trie over every phrase the parser knows ("nautical miles",
//! "as a % of", "$", "km²", "New York"), matched longest-first at a
//! position in a line. Matching folds case, and one space in a phrase
//! matches any run of whitespace in the input. Short symbols such as `M`
//! (million) versus `m` (metre) are stored case-sensitive and checked
//! against the original text.

use crate::vocab::Meaning;

#[derive(Debug, Default, Clone)]
struct Node {
    children: Vec<(char, u32)>,
    entries: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub phrase: String,
    pub case_sensitive: bool,
    pub meaning: Meaning,
}

#[derive(Debug, Clone)]
pub struct Trie {
    nodes: Vec<Node>,
    entries: Vec<Entry>,
}

/// A successful match: where it ends and every meaning the phrase has.
#[derive(Debug)]
pub struct Match {
    pub end: usize,
    pub meanings: Vec<Meaning>,
}

impl Default for Trie {
    fn default() -> Self {
        Self {
            nodes: vec![Node::default()],
            entries: Vec::new(),
        }
    }
}

/// Lowercase and strip common Latin accents, so `Sao Paulo` finds
/// `São Paulo` and `bolivar` finds `bolívar`.
fn fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let mut lower = c.to_lowercase();
    let lower = match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => return c,
    };
    match lower {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ś' | 'š' | 'ș' | 'ş' => 's',
        'ț' | 'ţ' | 'ť' => 't',
        'ź' | 'ż' | 'ž' => 'z',
        'ř' => 'r',
        'ď' => 'd',
        other => other,
    }
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Trie {
    /// Add a phrase. A phrase already present with the same meaning kind
    /// keeps its first meaning, so earlier tables win ties.
    pub fn insert(&mut self, phrase: &str, case_sensitive: bool, meaning: Meaning) {
        let phrase = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
        if phrase.is_empty() {
            return;
        }
        let node = self.walk_insert(&phrase);
        let duplicate = self.nodes[node].entries.iter().any(|&e| {
            let entry = &self.entries[e as usize];
            entry.meaning.kind() == meaning.kind()
                && (entry.phrase == phrase || (!entry.case_sensitive && !case_sensitive))
        });
        if duplicate {
            return;
        }
        let index = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        self.entries.push(Entry {
            phrase,
            case_sensitive,
            meaning,
        });
        self.nodes[node].entries.push(index);
    }

    /// Every phrase, in insertion order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether a phrase (any meaning) is known.
    pub fn contains(&self, phrase: &str) -> bool {
        self.longest_match(phrase, 0)
            .is_some_and(|m| m.end == phrase.len())
    }

    fn walk_insert(&mut self, phrase: &str) -> usize {
        let mut node = 0usize;
        for c in phrase.chars().map(fold) {
            let existing = self.nodes[node]
                .children
                .iter()
                .find(|(k, _)| *k == c)
                .map(|&(_, n)| n);
            node = match existing {
                Some(next) => next as usize,
                None => {
                    let next = self.nodes.len();
                    self.nodes.push(Node::default());
                    let id = u32::try_from(next).unwrap_or(u32::MAX);
                    self.nodes[node].children.push((c, id));
                    next
                }
            };
        }
        node
    }

    /// The longest phrase starting at byte `start` of `line` that ends on a
    /// word boundary. The caller checks the boundary before `start`.
    pub fn longest_match(&self, line: &str, start: usize) -> Option<Match> {
        let mut node = 0usize;
        let mut best = None;
        let mut chars = line[start..].char_indices().peekable();
        while let Some((offset, c)) = chars.next() {
            let key = if c.is_whitespace() { ' ' } else { fold(c) };
            let Some(&(_, next)) = self.nodes[node].children.iter().find(|(k, _)| *k == key) else {
                break;
            };
            node = next as usize;
            if c.is_whitespace() {
                while chars.peek().is_some_and(|(_, w)| w.is_whitespace()) {
                    chars.next();
                }
                continue;
            }
            let end = start + offset + c.len_utf8();
            if self.nodes[node].entries.is_empty() || !ends_on_boundary(line, end, c) {
                continue;
            }
            let text = &line[start..end];
            let meanings: Vec<Meaning> = self.nodes[node]
                .entries
                .iter()
                .map(|&e| &self.entries[e as usize])
                .filter(|e| !e.case_sensitive || same_words(&e.phrase, text))
                .map(|e| e.meaning)
                .collect();
            if !meanings.is_empty() {
                best = Some(Match { end, meanings });
            }
        }
        best
    }
}

fn ends_on_boundary(line: &str, end: usize, last: char) -> bool {
    if !is_word_char(last) {
        return true;
    }
    line[end..]
        .chars()
        .next()
        .is_none_or(|next| !is_word_char(next))
}

fn same_words(phrase: &str, text: &str) -> bool {
    phrase.split_whitespace().eq(text.split_whitespace())
}
