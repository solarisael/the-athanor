//! The live Orihon chat: a message-level binary tree of one-line nodes and the persisted
//! views over it. Sizes are UTF-8 bytes. Every function here is deterministic; storage and
//! scheduling live in `store` and in the Host.

pub mod store;

use hearth::orihon::{Kind, Record};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

/// The byte target of one node line. Short sources and joins fit it exactly.
pub const LINE_BYTES: usize = 512;
/// A model line over the target is kept after the compactor's size corrections, up to the
/// reply ceiling. Views measure the real bytes of every line.
pub const MODEL_LINE_CEILING: usize = 16 * 1024;
/// Non-echo text splits into units of at most this many bytes. Echo text arrives clipped.
pub const UNIT_BYTES: usize = 30_000;
pub const CHAT_HIGH: usize = 128_000;
pub const CHAT_LOW: usize = 64_000;
pub const COMPACTOR_HIGH: usize = 32_000;
pub const COMPACTOR_LOW: usize = 16_000;
pub const BLOCK_LINES: usize = 4;
/// A leaf starts once fewer than this many lines before it are unbuilt.
pub const LEAF_WINDOW: usize = 8;
pub const MAX_JOBS: usize = 8;
pub const MAX_LEVEL: u8 = 62;
pub const PLACEHOLDER: &str = "(not summarized yet: zoom it)";
pub const CHAT_OPEN: &str = "<chat>\n";
pub const CHAT_CLOSE: &str = "</chat>";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct NodeKey {
    pub level: u8,
    pub index: u64,
}

impl NodeKey {
    pub const fn leaf(unit: u64) -> Self {
        Self {
            level: 0,
            index: unit,
        }
    }

    /// The node `id+n` of the view, when `n` is a power of two and `id` a multiple of it.
    pub fn from_span(id: u64, n: u64) -> Option<Self> {
        if n == 0 || !n.is_power_of_two() || id % n != 0 {
            return None;
        }
        let level = n.trailing_zeros() as u8;
        (level <= MAX_LEVEL).then(|| Self {
            level,
            index: id >> level,
        })
    }

    pub const fn first(self) -> u64 {
        self.index << self.level
    }

    pub const fn count(self) -> u64 {
        1 << self.level
    }

    pub const fn end(self) -> u64 {
        self.first() + self.count()
    }

    pub const fn last(self) -> u64 {
        self.end() - 1
    }

    pub fn parent(self) -> Option<Self> {
        (self.level < MAX_LEVEL).then(|| Self {
            level: self.level + 1,
            index: self.index / 2,
        })
    }

    pub const fn sibling(self) -> Self {
        Self {
            level: self.level,
            index: self.index ^ 1,
        }
    }

    pub fn children(self) -> Option<[Self; 2]> {
        (self.level > 0).then(|| {
            let level = self.level - 1;
            [
                Self {
                    level,
                    index: self.index * 2,
                },
                Self {
                    level,
                    index: self.index * 2 + 1,
                },
            ]
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// A short unit is its own line.
    Verbatim,
    /// Two short lines joined by a newline.
    Joined,
    /// A model line from one accepted job.
    Model,
}

impl Method {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Verbatim => "verbatim",
            Self::Joined => "joined",
            Self::Model => "model",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "verbatim" => Some(Self::Verbatim),
            "joined" => Some(Self::Joined),
            "model" => Some(Self::Model),
            _ => None,
        }
    }
}

/// One stable contiguous chunk of a record. Units tile each record in seq order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Unit {
    pub unit: u64,
    pub seq: i64,
    pub part: u32,
    pub parts: u32,
    pub byte_start: usize,
    pub byte_end: usize,
    /// The unit reads the record's `foldText`, not its audit text.
    pub semantic: bool,
    pub record_digest: String,
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Node {
    pub key: NodeKey,
    pub text: String,
    pub method: Method,
    pub source_digest: String,
    pub digest: String,
    pub job: Option<i64>,
    pub built_at: String,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn kind_name(kind: Kind) -> &'static str {
    super::prepare::kind_name(kind)
}

/// The text a record contributes to the tree: `foldText` when the adapter supplied one.
pub fn unit_body(record: &Record) -> &str {
    record.fold_text.as_deref().unwrap_or(&record.text)
}

/// Byte ranges of the units of one body. Echo text stays one unit; other text splits at
/// `UNIT_BYTES`, preferring a newline in the second half of a window. The ranges tile the body.
pub fn split_body(body: &str, kind: Kind) -> Vec<(usize, usize)> {
    if kind == Kind::Echo || body.len() <= UNIT_BYTES {
        return vec![(0, body.len())];
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    while body.len() - start > UNIT_BYTES {
        let mut end = start + UNIT_BYTES;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        // The half-window may start inside a code point. A newline byte is
        // always a UTF-8 boundary, so search bytes rather than slicing text.
        let middle = start + UNIT_BYTES / 2;
        if let Some(newline) = body.as_bytes()[middle..end]
            .iter()
            .rposition(|byte| *byte == b'\n')
        {
            end = middle + newline + 1;
        }
        ranges.push((start, end));
        start = end;
    }
    ranges.push((start, body.len()));
    ranges
}

/// The units of one record, numbered from `first_unit`.
pub fn units_for(record: &Record, first_unit: u64) -> Vec<Unit> {
    let body = unit_body(record);
    let ranges = split_body(body, record.kind);
    let parts = ranges.len() as u32;
    ranges
        .into_iter()
        .enumerate()
        .map(|(part, (start, end))| Unit {
            unit: first_unit + part as u64,
            seq: record.seq,
            part: part as u32,
            parts,
            byte_start: start,
            byte_end: end,
            semantic: record.fold_text.is_some(),
            record_digest: record.content_digest.clone(),
            digest: sha256_hex(&body.as_bytes()[start..end]),
        })
        .collect()
}

/// The input a leaf compresses: its kind, its part when the record has several, and its text.
pub fn leaf_input(kind: Kind, unit: &Unit, text: &str) -> String {
    if unit.parts > 1 {
        format!(
            "{} (part {} of {}): {text}",
            kind_name(kind),
            unit.part + 1,
            unit.parts
        )
    } else {
        format!("{}: {text}", kind_name(kind))
    }
}

/// A short input is its own line, with no model.
pub fn verbatim(input: &str) -> Option<&str> {
    (!input.is_empty() && input.len() <= LINE_BYTES).then_some(input)
}

/// Two built lines that fit one line together are joined, with no model.
pub fn joined(left: &str, right: &str) -> Option<String> {
    (left.len() + 1 + right.len() <= LINE_BYTES).then(|| format!("{left}\n{right}"))
}

pub fn branch_source_digest(left: &str, right: &str) -> String {
    sha256_hex(format!("{left}:{right}").as_bytes())
}

/// Node identity over geometry, exact sources, and text. PostgreSQL recomputes it on insert.
pub fn node_digest(key: NodeKey, source_digest: &str, text: &str) -> String {
    sha256_hex(
        format!(
            "{}:{}:{source_digest}:{}",
            key.level,
            key.index,
            sha256_hex(text.as_bytes())
        )
        .as_bytes(),
    )
}

/// A view line: `id+n|text`, with newlines as spaces, then a newline.
pub fn render_line(key: NodeKey, text: Option<&str>) -> String {
    let body = text.map_or_else(
        || PLACEHOLDER.to_owned(),
        |text| text.replace(['\n', '\r'], " "),
    );
    format!("{}+{}|{body}\n", key.first(), key.count())
}

pub fn line_bytes(key: NodeKey, text_bytes: Option<usize>) -> usize {
    let digits = |value: u64| value.checked_ilog10().map_or(1, |log| log as usize + 1);
    digits(key.first()) + 1 + digits(key.count()) + 1 + text_bytes.unwrap_or(PLACEHOLDER.len()) + 1
}

/// Built node sizes. `None` means the node is not built.
pub trait Sizes {
    fn bytes(&self, key: NodeKey) -> Option<usize>;
}

impl<S: std::hash::BuildHasher> Sizes for std::collections::HashMap<NodeKey, usize, S> {
    fn bytes(&self, key: NodeKey) -> Option<usize> {
        self.get(&key).copied()
    }
}

/// How long ago a pair ended, in its own line size: `(T - last) / 2^level`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Due {
    since: u64,
    level: u8,
}

impl Ord for Due {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = u128::from(self.since) << other.level;
        let right = u128::from(other.since) << self.level;
        left.cmp(&right)
    }
}

impl PartialOrd for Due {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One persisted view: lines that cover units `0..units` oldest first, and its batch state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct View {
    pub revision: u64,
    pub units: u64,
    /// A batch passed the high watermark and has not reached the low one yet.
    pub merging: bool,
    #[serde(with = "line_pairs")]
    pub lines: Vec<NodeKey>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            revision: 1,
            units: 0,
            merging: false,
            lines: Vec::new(),
        }
    }
}

mod line_pairs {
    use super::NodeKey;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(lines: &[NodeKey], serializer: S) -> Result<S::Ok, S::Error> {
        lines
            .iter()
            .map(|key| (key.level, key.index))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<NodeKey>, D::Error> {
        Ok(Vec::<(u8, u64)>::deserialize(deserializer)?
            .into_iter()
            .map(|(level, index)| NodeKey { level, index })
            .collect())
    }
}

impl View {
    pub fn bytes(&self, sizes: &impl Sizes) -> usize {
        self.lines
            .iter()
            .map(|key| line_bytes(*key, sizes.bytes(*key)))
            .sum()
    }

    /// Lines tile `0..units` with aligned tree nodes, oldest first.
    pub fn validate(&self) -> Result<(), String> {
        let mut next = 0;
        for key in &self.lines {
            if key.level > MAX_LEVEL || key.first() != next {
                return Err(format!(
                    "view line {}+{} leaves a gap or overlap",
                    key.first(),
                    key.count()
                ));
            }
            next = key.end();
        }
        if next != self.units {
            return Err(format!("view covers {next} units, not {}", self.units));
        }
        Ok(())
    }

    /// Each new unit appends its line. Nothing else changes.
    pub fn append(&mut self, units: u64) -> bool {
        if units <= self.units {
            return false;
        }
        self.lines.extend((self.units..units).map(NodeKey::leaf));
        self.units = units;
        true
    }

    /// Merge the most due sibling pair whose parent is built, oldest first on ties, until the
    /// view fits `target` or no pair can merge. Returns the merge count.
    pub fn merge_down(&mut self, total: u64, target: usize, sizes: &impl Sizes) -> usize {
        let mut bytes = self.bytes(sizes);
        let mut merges = 0;
        while bytes > target {
            let mut best: Option<(usize, Due)> = None;
            for position in 0..self.lines.len().saturating_sub(1) {
                let (left, right) = (self.lines[position], self.lines[position + 1]);
                if left.level != right.level || left.index % 2 != 0 || right.index != left.index + 1
                {
                    continue;
                }
                let Some(parent) = left.parent() else {
                    continue;
                };
                if sizes.bytes(parent).is_none() {
                    continue;
                }
                let due = Due {
                    since: total.saturating_sub(right.last()),
                    level: left.level,
                };
                // Iteration runs oldest first, so a strict comparison keeps the oldest tie.
                if best.is_none_or(|(_, current)| due > current) {
                    best = Some((position, due));
                }
            }
            let Some((position, _)) = best else { break };
            let (left, right) = (self.lines[position], self.lines[position + 1]);
            let parent = left.parent().expect("candidate has a parent");
            bytes =
                bytes - line_bytes(left, sizes.bytes(left)) - line_bytes(right, sizes.bytes(right))
                    + line_bytes(parent, sizes.bytes(parent));
            self.lines.splice(position..position + 2, [parent]);
            merges += 1;
        }
        merges
    }

    /// Lines that end at or before `end`, stopping at the first unbuilt line. Compactions see
    /// only built lines.
    pub fn built_prefix(&self, end: u64, sizes: &impl Sizes) -> Vec<NodeKey> {
        self.lines
            .iter()
            .copied()
            .take_while(|key| key.end() <= end && sizes.bytes(*key).is_some())
            .collect()
    }

    /// The view cut exactly at unit `end`. A line that crosses `end` shows as its descendants.
    /// Every returned line must be built; the first unbuilt line is the refusal.
    pub fn cut(&self, end: u64, sizes: &impl Sizes) -> Result<Vec<NodeKey>, NodeKey> {
        let mut lines = Vec::new();
        for key in &self.lines {
            if key.first() >= end {
                break;
            }
            let mut pending = vec![*key];
            while let Some(key) = pending.pop() {
                if key.end() <= end {
                    if sizes.bytes(key).is_none() {
                        return Err(key);
                    }
                    lines.push(key);
                } else if let Some([left, right]) = key.children() {
                    if right.first() < end {
                        pending.push(right);
                    }
                    pending.push(left);
                }
            }
        }
        Ok(lines)
    }
}

/// What one message boundary changed.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct Maintenance {
    pub appended: bool,
    pub chat_merges: usize,
    pub compactor_merges: usize,
}

impl Maintenance {
    pub fn changed(&self) -> bool {
        self.appended || self.chat_merges > 0 || self.compactor_merges > 0
    }
}

/// The message-boundary step for both persisted views. New units append their lines. Past 128 KB
/// the chat view merges down to 64 KB, continuing at later boundaries while parents are unbuilt.
/// The compactor view independently merges from 32 KB down to 16 KB.
pub fn maintain(
    chat: &mut View,
    compactor: &mut View,
    units: u64,
    sizes: &impl Sizes,
) -> Maintenance {
    let mut change = Maintenance {
        appended: chat.append(units),
        ..Maintenance::default()
    };
    compactor.append(units);
    if !chat.merging && chat.bytes(sizes) > CHAT_HIGH {
        chat.merging = true;
    }
    if chat.merging {
        change.chat_merges = chat.merge_down(units, CHAT_LOW, sizes);
        chat.merging = chat.bytes(sizes) > CHAT_LOW;
    }
    if !compactor.merging && compactor.bytes(sizes) > COMPACTOR_HIGH {
        compactor.merging = true;
    }
    if compactor.merging {
        change.compactor_merges = compactor.merge_down(units, COMPACTOR_LOW, sizes);
        compactor.merging = compactor.bytes(sizes) > COMPACTOR_LOW;
    }
    if change.changed() {
        chat.revision += 1;
        compactor.revision += 1;
    }
    change
}

/// The exact view text as whole 4-line blocks plus a tail. `concat(blocks) + tail` is the text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub blocks: Vec<String>,
    pub tail: String,
    pub bytes: usize,
    pub lines: usize,
}

impl Rendered {
    pub fn text(&self) -> String {
        let mut text = self.blocks.concat();
        text.push_str(&self.tail);
        text
    }
}

pub fn render(lines: &[(NodeKey, &str)]) -> Rendered {
    let mut blocks = Vec::with_capacity(lines.len() / BLOCK_LINES);
    let whole = lines.len() - lines.len() % BLOCK_LINES;
    for (number, chunk) in lines[..whole].chunks(BLOCK_LINES).enumerate() {
        let mut block = if number == 0 {
            CHAT_OPEN.to_owned()
        } else {
            String::new()
        };
        for (key, text) in chunk {
            block.push_str(&render_line(*key, Some(text)));
        }
        blocks.push(block);
    }
    let mut tail = if whole == 0 {
        CHAT_OPEN.to_owned()
    } else {
        String::new()
    };
    for (key, text) in &lines[whole..] {
        tail.push_str(&render_line(*key, Some(text)));
    }
    tail.push_str(CHAT_CLOSE);
    let bytes = blocks.iter().map(String::len).sum::<usize>() + tail.len();
    Rendered {
        blocks,
        tail,
        bytes,
        lines: lines.len(),
    }
}

/// The unit end of a seq frontier: every unit of records at or before `seq`.
pub fn units_through(units: &[Unit], seq: i64) -> u64 {
    units
        .iter()
        .take_while(|unit| unit.seq <= seq)
        .last()
        .map_or(0, |unit| unit.unit + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn built(keys: &[(u8, u64, usize)]) -> HashMap<NodeKey, usize> {
        keys.iter()
            .map(|(level, index, bytes)| {
                (
                    NodeKey {
                        level: *level,
                        index: *index,
                    },
                    *bytes,
                )
            })
            .collect()
    }

    #[test]
    fn due_merges_the_newest_pair_at_t10_like_push() {
        // View 0+4, 4+4, 8+1, 9+1 at T=10, with both candidate parents built.
        let sizes = built(&[
            (2, 0, 400),
            (2, 1, 400),
            (0, 8, 400),
            (0, 9, 400),
            (3, 0, 400),
            (1, 4, 400),
        ]);
        let mut view = View {
            lines: vec![key(2, 0), key(2, 1), key(0, 8), key(0, 9)],
            units: 10,
            ..View::default()
        };
        let target = view.bytes(&sizes) - 1;
        assert_eq!(view.merge_down(10, target, &sizes), 1);
        // Push merges 8-9 next, never 0-7.
        assert_eq!(view.lines, vec![key(2, 0), key(2, 1), key(1, 4)]);
        assert_eq!(view.merge_down(10, 0, &sizes), 1);
        assert_eq!(view.lines, vec![key(3, 0), key(1, 4)]);
    }

    #[test]
    fn equal_due_pairs_merge_oldest_first() {
        // At T=7 the pair 0+2,2+2 ended (7-3)/2 = 2 lines ago and 4+1,5+1 ended (7-5)/1 = 2.
        let mut view = View {
            lines: vec![key(1, 0), key(1, 1), key(0, 4), key(0, 5), key(0, 6)],
            units: 7,
            ..View::default()
        };
        let sizes = built(&[
            (1, 0, 100),
            (1, 1, 100),
            (0, 4, 100),
            (0, 5, 100),
            (0, 6, 100),
            (2, 0, 100),
            (1, 2, 100),
        ]);
        let target = view.bytes(&sizes) - 1;
        assert_eq!(view.merge_down(7, target, &sizes), 1);
        assert_eq!(view.lines, vec![key(2, 0), key(0, 4), key(0, 5), key(0, 6)]);
    }

    fn key(level: u8, index: u64) -> NodeKey {
        NodeKey { level, index }
    }

    #[test]
    fn unbuilt_parents_never_merge_and_coverage_stays_whole() {
        let mut view = View::default();
        view.append(6);
        let sizes = built(&[
            (0, 0, 500),
            (0, 1, 500),
            (0, 2, 500),
            (0, 3, 500),
            (0, 4, 500),
        ]);
        assert_eq!(view.merge_down(6, 0, &sizes), 0);
        view.validate().unwrap();
        let cut = view.cut(5, &sizes).unwrap();
        assert_eq!(cut.len(), 5);
        assert_eq!(view.cut(6, &sizes), Err(key(0, 5)));
        assert_eq!(view.built_prefix(6, &sizes).len(), 5);
    }

    #[test]
    fn batch_runs_from_high_to_low_and_appends_keep_the_prefix_between_batches() {
        let mut chat = View::default();
        let mut compactor = View::default();
        let mut sizes = HashMap::new();
        let mut previous_text = String::new();
        let mut batches = 0;
        for units in 1..=2_000u64 {
            let leaf = NodeKey::leaf(units - 1);
            sizes.insert(leaf, 480);
            // Build every complete parent as the compactor would.
            let mut node = leaf;
            while node.index % 2 == 1 {
                let parent = node.parent().unwrap();
                sizes.insert(parent, 500);
                node = parent;
            }
            let before = chat.bytes(&sizes);
            let change = maintain(&mut chat, &mut compactor, units, &sizes);
            chat.validate().unwrap();
            compactor.validate().unwrap();
            let text: String = chat
                .lines
                .iter()
                .map(|key| render_line(*key, Some(&"x".repeat(sizes[key]))))
                .collect();
            if change.chat_merges > 0 {
                batches += 1;
                assert!(
                    before + 600 > CHAT_HIGH,
                    "a batch starts only past the high watermark"
                );
                assert!(chat.bytes(&sizes) <= CHAT_LOW);
            } else if !previous_text.is_empty() {
                assert!(
                    text.starts_with(&previous_text),
                    "between batches the view only grows"
                );
                assert!(chat.bytes(&sizes) <= CHAT_HIGH + 600);
            }
            assert!(compactor.bytes(&sizes) <= COMPACTOR_HIGH + 600);
            previous_text = text;
        }
        assert!(batches >= 2);
    }

    #[test]
    fn persisted_view_reloads_byte_identical() {
        let mut view = View::default();
        view.append(9);
        view.revision = 7;
        view.merging = true;
        let sizes = built(&[(1, 0, 10), (0, 0, 5), (0, 1, 5)]);
        view.merge_down(9, 0, &sizes);
        let json = serde_json::to_string(&view).unwrap();
        let reloaded: View = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded, view);
        assert_eq!(serde_json::to_string(&reloaded).unwrap(), json);
        let lines: Vec<_> = reloaded
            .lines
            .iter()
            .map(|key| (*key, "same text"))
            .collect();
        let original: Vec<_> = view.lines.iter().map(|key| (*key, "same text")).collect();
        assert_eq!(render(&lines).text(), render(&original).text());
    }

    #[test]
    fn chat_only_merge_preserves_compactor_parts_until_its_own_batch() {
        let texts: HashMap<_, _> = (0..=8)
            .flat_map(|level| {
                (0..(320 >> level)).map(move |index| (key(level, index), "é".repeat(255)))
            })
            .collect();
        let sizes: HashMap<_, _> = texts.iter().map(|(key, text)| (*key, text.len())).collect();
        let rendered = |view: &View| {
            let lines: Vec<_> = view
                .lines
                .iter()
                .map(|key| (*key, texts[key].as_str()))
                .collect();
            render(&lines)
        };
        let mut chat = View::default();
        chat.append(256);
        let mut compactor = View {
            units: 256,
            lines: (0..8).map(|index| key(5, index)).collect(),
            ..View::default()
        };
        chat.validate().unwrap();
        compactor.validate().unwrap();
        assert!(chat.bytes(&sizes) > CHAT_HIGH);
        assert!(compactor.bytes(&sizes) < COMPACTOR_LOW);

        let prefix = compactor.lines.clone();
        let before = rendered(&compactor);
        let revision = compactor.revision;
        let change = maintain(&mut chat, &mut compactor, 256, &sizes);

        assert_eq!(rendered(&compactor), before);
        assert_eq!(compactor.lines, prefix);
        assert!(change.chat_merges > 0);
        assert_eq!(change.compactor_merges, 0);
        assert!(!compactor.merging);
        assert!(compactor.revision > revision);
        assert!(chat.bytes(&sizes) <= CHAT_LOW);
        chat.validate().unwrap();
        compactor.validate().unwrap();

        let change = maintain(&mut chat, &mut compactor, 257, &sizes);

        assert!(change.appended);
        assert_eq!(change.chat_merges, 0);
        assert_eq!(change.compactor_merges, 0);
        assert_eq!(&compactor.lines[..prefix.len()], prefix.as_slice());
        assert_eq!(compactor.lines.last(), Some(&NodeKey::leaf(256)));
        assert_eq!(rendered(&compactor).blocks, before.blocks);
        assert_eq!(
            rendered(&compactor).text(),
            format!(
                "{}{}{}",
                before.text().strip_suffix(CHAT_CLOSE).unwrap(),
                render_line(NodeKey::leaf(256), Some(&texts[&NodeKey::leaf(256)])),
                CHAT_CLOSE
            )
        );
        assert_eq!(
            rendered(&compactor).bytes,
            compactor.bytes(&sizes) + CHAT_OPEN.len() + CHAT_CLOSE.len()
        );

        let change = maintain(&mut chat, &mut compactor, 320, &sizes);

        assert!(change.appended);
        assert_eq!(change.chat_merges, 0);
        assert!(change.compactor_merges > 0);
        assert!(compactor.bytes(&sizes) <= COMPACTOR_LOW);
        assert!(!compactor.merging);
        chat.validate().unwrap();
        compactor.validate().unwrap();
        assert_eq!(compactor.units, 320);
        assert!(compactor.lines.iter().all(|key| texts.contains_key(key)));
        let cut = compactor.cut(319, &sizes).unwrap();
        let frontier = View {
            units: 319,
            lines: cut,
            ..View::default()
        };
        frontier.validate().unwrap();

        let persisted = serde_json::to_string(&compactor).unwrap();
        let mut reloaded: View = serde_json::from_str(&persisted).unwrap();

        assert_eq!(reloaded, compactor);
        assert_eq!(rendered(&reloaded), rendered(&compactor));
        let stable = rendered(&reloaded);
        let change = maintain(&mut chat, &mut reloaded, 320, &sizes);

        assert!(!change.changed());
        assert_eq!(rendered(&reloaded), stable);
    }

    #[test]
    fn frontier_cut_shows_a_crossing_line_as_its_built_descendants() {
        let view = View {
            lines: vec![key(3, 0), key(0, 8)],
            units: 9,
            ..View::default()
        };
        let mut sizes = built(&[
            (3, 0, 50),
            (2, 0, 50),
            (2, 1, 50),
            (1, 2, 50),
            (1, 3, 50),
            (0, 6, 50),
            (0, 7, 50),
            (0, 8, 50),
        ]);
        sizes.insert(key(0, 4), 50);
        sizes.insert(key(0, 5), 50);
        // The current user unit is 7: everything before it, nothing after.
        let cut = view.cut(7, &sizes).unwrap();
        assert_eq!(cut, vec![key(2, 0), key(1, 2), key(0, 6)]);
        assert!(cut.iter().all(|line| line.end() <= 7));
        assert_eq!(cut.last().unwrap().end(), 7);
    }

    #[test]
    fn source_cutoff_excludes_the_latest_user_record() {
        let record = |seq: i64, text: &str| Record {
            room: "proof".into(),
            seq,
            turn_no: 0,
            session_id: "s".into(),
            harness: "omp".into(),
            source_session: "s".into(),
            source_id: format!("id-{seq}"),
            parent_user_id: None,
            kind: Kind::User,
            speaker: "operator".into(),
            text: text.into(),
            said_at: "2026-10-23T00:00:00Z".into(),
            timestamp_observed: false,
            written_at: "2026-10-23T00:00:00Z".into(),
            truncated: false,
            redacted: false,
            content_digest: "0".repeat(64),
            fold_text: None,
            children: None,
        };
        let mut units = units_for(&record(0, "first"), 0);
        units.extend(units_for(&record(1, &"long ".repeat(7_000)), 1));
        units.extend(units_for(
            &record(2, "latest user words"),
            units.len() as u64,
        ));
        let through = units_through(&units, 1);
        assert_eq!(through, units.len() as u64 - 1);
        assert!(
            units
                .iter()
                .filter(|unit| unit.unit < through)
                .all(|unit| unit.seq <= 1)
        );
    }

    #[test]
    fn long_utf8_split_is_stable_lossless_and_bounded() {
        let text: String = (0..40_000)
            .map(|index| ['a', 'é', '漢', '🦀', '\n'][index % 5])
            .collect();
        let first = split_body(&text, Kind::Talk);
        assert_eq!(first, split_body(&text, Kind::Talk));
        let mut rebuilt = String::new();
        let mut next = 0;
        for (start, end) in &first {
            assert_eq!(*start, next);
            assert!(text.is_char_boundary(*start) && text.is_char_boundary(*end));
            assert!(end - start <= UNIT_BYTES);
            rebuilt.push_str(&text[*start..*end]);
            next = *end;
        }
        assert_eq!(rebuilt, text);
        assert_eq!(split_body(&text, Kind::Echo), vec![(0, text.len())]);
        assert_eq!(split_body("", Kind::Talk), vec![(0, 0)]);
    }

    #[test]
    fn blocks_hold_four_whole_lines_and_concatenate_to_the_view() {
        let lines: Vec<_> = (0..10).map(|unit| (NodeKey::leaf(unit), "line")).collect();
        let rendered = render(&lines);
        assert_eq!(rendered.blocks.len(), 2);
        assert!(rendered.blocks[0].starts_with(CHAT_OPEN));
        assert!(rendered.tail.ends_with(CHAT_CLOSE));
        assert_eq!(rendered.text().len(), rendered.bytes);
        assert_eq!(rendered.text().matches('\n').count(), 11);
        assert_eq!(
            line_bytes(NodeKey::leaf(9), Some(4)),
            render_line(NodeKey::leaf(9), Some("line")).len()
        );
        assert_eq!(
            line_bytes(key(10, 1), None),
            render_line(key(10, 1), None).len()
        );
    }

    #[test]
    fn spans_accept_only_aligned_powers_of_two() {
        assert_eq!(NodeKey::from_span(40, 8), Some(key(3, 5)));
        assert_eq!(NodeKey::from_span(41, 8), None);
        assert_eq!(NodeKey::from_span(40, 6), None);
        assert_eq!(key(3, 5).first(), 40);
        assert_eq!(key(3, 5).count(), 8);
    }
}
