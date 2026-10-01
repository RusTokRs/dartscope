//! Statement boundaries, angle brackets and braces of one masked source text.
//!
//! Several heuristics of the reference passes look backwards to the start of the statement around a
//! token, count the angle brackets in front of it, or look for the brace block that encloses a
//! declaration. Done per token with a scan, each of them costs the length of the statement or of the
//! enclosing body, so a long statement or a long method makes the file quadratic. One pass over the
//! text records where the interesting bytes are, and each question becomes a binary search that
//! returns what the scan returned.

use crate::interval_index::MinTree;

/// Positions of the bytes that the statement heuristics care about, found in one pass.
pub(crate) struct SourceStructure {
    len: usize,
    /// Positions of `;`, `{` and `}`, in increasing order.
    boundaries: Vec<usize>,
    /// Positions of `>`, in increasing order.
    closing_angles: Vec<usize>,
    /// Depth of unclosed `<` after every `<`, `>` and statement boundary that changes it, as
    /// `(position, depth)`. Depth restarts at zero after each boundary and never goes below it.
    angle_depths: Vec<(usize, usize)>,
    /// Positions of every `{`, in increasing order, and the position of the `}` that closes each.
    brace_opens: Vec<usize>,
    brace_closes: Vec<Option<usize>>,
    /// After each brace byte, the position of the innermost `{` that is still open.
    brace_tops: Vec<(usize, Option<usize>)>,
    /// The bytes that can end the scan of an expression (`;`, `,`, `{` and `}`) with the key that
    /// tells for which start of a scan they do; see `expression_end`.
    breaks: Vec<(usize, usize)>,
    break_keys: MinTree,
}

impl SourceStructure {
    pub(crate) fn new(source: &str) -> Self {
        let mut structure = Self {
            len: source.len(),
            boundaries: Vec::new(),
            closing_angles: Vec::new(),
            angle_depths: Vec::new(),
            brace_opens: Vec::new(),
            brace_closes: Vec::new(),
            brace_tops: Vec::new(),
            breaks: Vec::new(),
            break_keys: MinTree::new(&[]),
        };
        let mut depth = 0usize;
        let mut open_blocks: Vec<usize> = Vec::new();
        let mut open_parens: Vec<usize> = Vec::new();
        let mut open_brackets: Vec<usize> = Vec::new();
        for (at, byte) in source.bytes().enumerate() {
            match byte {
                b'(' => open_parens.push(at),
                b')' => {
                    open_parens.pop();
                }
                b'[' => open_brackets.push(at),
                b']' => {
                    open_brackets.pop();
                }
                b',' => {
                    let innermost = innermost_open(&open_parens, &open_brackets, &open_blocks, &structure.brace_opens);
                    structure.breaks.push((at, innermost));
                }
                b'<' => {
                    depth += 1;
                    structure.angle_depths.push((at, depth));
                }
                b'>' => {
                    depth = depth.saturating_sub(1);
                    structure.angle_depths.push((at, depth));
                    structure.closing_angles.push(at);
                }
                b';' | b'{' | b'}' => {
                    // A closing brace ends a scan whenever no block opened since the scan started
                    // is open; the other bytes also need every parenthesis and bracket closed.
                    let brace_top = open_blocks
                        .last()
                        .map_or(0, |&index| structure.brace_opens[index] + 1);
                    let key = if byte == b'}' {
                        brace_top
                    } else {
                        innermost_open(&open_parens, &open_brackets, &open_blocks, &structure.brace_opens)
                    };
                    structure.breaks.push((at, key));
                    if depth > 0 {
                        depth = 0;
                        structure.angle_depths.push((at, 0));
                    }
                    structure.boundaries.push(at);
                    match byte {
                        b'{' => {
                            open_blocks.push(structure.brace_opens.len());
                            structure.brace_opens.push(at);
                            structure.brace_closes.push(None);
                        }
                        b'}' => {
                            if let Some(index) = open_blocks.pop() {
                                structure.brace_closes[index] = Some(at);
                            }
                        }
                        _ => {}
                    }
                    if byte != b';' {
                        let top = open_blocks
                            .last()
                            .map(|&index| structure.brace_opens[index]);
                        structure.brace_tops.push((at, top));
                    }
                }
                _ => {}
            }
        }
        let keys: Vec<usize> = structure.breaks.iter().map(|&(_, key)| key).collect();
        structure.break_keys = MinTree::new(&keys);
        structure
    }

    /// The offset just after the last `;`, `{` or `}` that precedes `before`; zero when there is
    /// none.
    pub(crate) fn statement_start(&self, before: usize) -> usize {
        let before = before.min(self.len);
        match self.boundaries.partition_point(|&at| at < before) {
            0 => 0,
            count => self.boundaries[count - 1] + 1,
        }
    }

    /// Whether `[left, right)` (in either order) is inside the text and holds no `;`, `{` or `}`.
    pub(crate) fn has_no_statement_boundary_between(&self, left: usize, right: usize) -> bool {
        let (start, end) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        if end > self.len {
            return false;
        }
        self.boundaries
            .get(self.boundaries.partition_point(|&at| at < start))
            .is_none_or(|&at| at >= end)
    }

    /// Whether `offset` lies between the `<` and the `>` of a type-argument-like pair that opens
    /// earlier in its statement and closes before the statement ends.
    pub(crate) fn is_inside_angle_pair(&self, offset: usize) -> bool {
        let offset = offset.min(self.len);
        let depth = match self.angle_depths.partition_point(|&(at, _)| at < offset) {
            0 => 0,
            count => self.angle_depths[count - 1].1,
        };
        if depth == 0 {
            return false;
        }
        // The pair closes when `depth` more `>` follow before the next statement boundary.
        let first = self.closing_angles.partition_point(|&at| at < offset);
        let Some(&closing) = self.closing_angles.get(first + depth - 1) else {
            return false;
        };
        self.boundaries
            .get(self.boundaries.partition_point(|&at| at < offset))
            .is_none_or(|&boundary| closing < boundary)
    }

    /// The innermost `{` before `limit` that is still open at `limit`, counting only braces at or
    /// after `floor`.
    pub(crate) fn innermost_open_brace(&self, floor: usize, limit: usize) -> Option<usize> {
        let count = self.brace_tops.partition_point(|&(at, _)| at < limit);
        let (_, top) = *self.brace_tops.get(count.checked_sub(1)?)?;
        top.filter(|&open| open >= floor)
    }

    /// Where a scan of an expression that begins at `start` stops: at the first `;`, `,` or `{`
    /// that is outside every group opened since `start`, or at the first `}` that closes a block
    /// opened before `start`; `None` when it runs to the end of the text. A `)` or `]` that
    /// closes a group opened before `start` does not stop the scan.
    pub(crate) fn expression_end(&self, start: usize) -> Option<usize> {
        let from = self.breaks.partition_point(|&(at, _)| at < start);
        let index = self.break_keys.first_at_or_below(from, start)?;
        Some(self.breaks[index].0)
    }

    /// The `}` that closes the `{` at `open`.
    pub(crate) fn closing_brace(&self, open: usize) -> Option<usize> {
        let index = self.brace_opens.binary_search(&open).ok()?;
        self.brace_closes[index]
    }
}

/// The key of a break at a position where the innermost open delimiter of any kind is the
/// parenthesis, bracket or block that opened last: one more than its position, so that zero means
/// there is none and the break ends every scan.
fn innermost_open(
    parens: &[usize],
    brackets: &[usize],
    blocks: &[usize],
    brace_opens: &[usize],
) -> usize {
    let paren = parens.last().map_or(0, |&at| at + 1);
    let bracket = brackets.last().map_or(0, |&at| at + 1);
    let brace = blocks.last().map_or(0, |&index| brace_opens[index] + 1);
    paren.max(bracket).max(brace)
}

#[cfg(test)]
mod tests {
    use super::SourceStructure;

    /// The scans that the structure replaces, kept as the specification.
    mod linear {
        pub(super) fn statement_start(source: &str, before: usize) -> usize {
            let bytes = source.as_bytes();
            let mut at = before.min(bytes.len());
            while at > 0 {
                at -= 1;
                if matches!(bytes[at], b';' | b'{' | b'}') {
                    return at + 1;
                }
            }
            0
        }

        pub(super) fn has_no_statement_boundary_between(
            source: &str,
            left: usize,
            right: usize,
        ) -> bool {
            let (start, end) = if left <= right {
                (left, right)
            } else {
                (right, left)
            };
            source
                .as_bytes()
                .get(start..end)
                .is_some_and(|bytes| !bytes.iter().any(|byte| matches!(*byte, b';' | b'{' | b'}')))
        }

        pub(super) fn is_inside_angle_pair(source: &str, offset: usize) -> bool {
            let bytes = source.as_bytes();
            let start = statement_start(source, offset);
            let mut depth = 0usize;
            for byte in &bytes[start..offset.min(bytes.len())] {
                match byte {
                    b'<' => depth += 1,
                    b'>' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if depth == 0 {
                return false;
            }
            for byte in &bytes[offset.min(bytes.len())..] {
                match byte {
                    b'>' => {
                        depth -= 1;
                        if depth == 0 {
                            return true;
                        }
                    }
                    b';' | b'{' | b'}' if depth > 0 => return false,
                    _ => {}
                }
            }
            false
        }

        /// Where the scan of an expression from `start` stops; `None` at the end of the text.
        pub(super) fn expression_end(source: &str, start: usize) -> Option<usize> {
            let bytes = source.as_bytes();
            let mut at = start;
            let mut parens = 0usize;
            let mut brackets = 0usize;
            let mut braces = 0usize;
            while at < bytes.len() {
                match bytes[at] {
                    b'(' => parens += 1,
                    b')' => parens = parens.saturating_sub(1),
                    b'[' => brackets += 1,
                    b']' => brackets = brackets.saturating_sub(1),
                    b'{' if parens == 0 && brackets == 0 && braces == 0 => return Some(at),
                    b'{' => braces += 1,
                    b'}' if braces == 0 => return Some(at),
                    b'}' => braces -= 1,
                    b',' | b';' if parens == 0 && brackets == 0 && braces == 0 => {
                        return Some(at);
                    }
                    _ => {}
                }
                at += 1;
            }
            None
        }

        /// The innermost block that is open at `limit`, found by scanning from `floor`.
        pub(super) fn innermost_open_brace(
            source: &str,
            floor: usize,
            limit: usize,
        ) -> Option<usize> {
            let bytes = source.as_bytes();
            let mut blocks = Vec::new();
            let mut at = floor;
            while at < limit.min(bytes.len()) {
                match bytes[at] {
                    b'{' => blocks.push(at),
                    b'}' => {
                        blocks.pop();
                    }
                    _ => {}
                }
                at += 1;
            }
            blocks.last().copied()
        }

        pub(super) fn closing_brace(source: &str, open: usize, limit: usize) -> Option<usize> {
            let bytes = source.as_bytes();
            if bytes.get(open) != Some(&b'{') {
                return None;
            }
            let mut depth = 1usize;
            let mut at = open + 1;
            while at < limit.min(bytes.len()) {
                match bytes[at] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(at);
                        }
                    }
                    _ => {}
                }
                at += 1;
            }
            None
        }
    }

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            let value = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
            usize::try_from(value % bound as u64).unwrap_or(0)
        }
    }

    fn random_source(rng: &mut Rng) -> String {
        const PIECES: &[&str] = &[
            "<", ">", "<", ">", ";", "{", "}", "{", "}", "(", ")", "[", "]", "a", "bc", " ", "\n", "=>", ", ",
        ];
        let count = rng.below(24);
        (0..count)
            .map(|_| PIECES[rng.below(PIECES.len())])
            .collect()
    }

    #[test]
    fn answers_agree_with_the_scans_they_replace() {
        let mut rng = Rng(0xDEAD_BEEF_0BAD_F00D);
        for _ in 0..400 {
            let source = random_source(&mut rng);
            let structure = SourceStructure::new(&source);
            let len = source.len();
            for at in 0..=len + 2 {
                assert_eq!(
                    structure.statement_start(at),
                    linear::statement_start(&source, at),
                    "statement_start({at}) in {source:?}"
                );
                assert_eq!(
                    structure.expression_end(at),
                    linear::expression_end(&source, at),
                    "expression_end({at}) in {source:?}"
                );
                if at <= len {
                    assert_eq!(
                        structure.is_inside_angle_pair(at),
                        linear::is_inside_angle_pair(&source, at),
                        "is_inside_angle_pair({at}) in {source:?}"
                    );
                }
                for other in 0..=len + 2 {
                    assert_eq!(
                        structure.has_no_statement_boundary_between(at, other),
                        linear::has_no_statement_boundary_between(&source, at, other),
                        "has_no_statement_boundary_between({at}, {other}) in {source:?}"
                    );
                    assert_eq!(
                        structure.innermost_open_brace(at, other),
                        linear::innermost_open_brace(&source, at, other),
                        "innermost_open_brace({at}, {other}) in {source:?}"
                    );
                }
            }
            for open in 0..len {
                for limit in [open + 1, len, len + 3] {
                    assert_eq!(
                        structure
                            .closing_brace(open)
                            .filter(|&close| close < limit.min(len)),
                        linear::closing_brace(&source, open, limit),
                        "closing_brace({open}) below {limit} in {source:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_closing_brace_without_an_opening_one_changes_nothing() {
        let source = "} a { b } } c {";
        let structure = SourceStructure::new(source);
        assert_eq!(structure.innermost_open_brace(0, source.len()), Some(14));
        assert_eq!(structure.closing_brace(4), Some(8));
        assert_eq!(structure.closing_brace(14), None);
        assert_eq!(structure.closing_brace(0), None);
    }
}
