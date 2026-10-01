//! Matching delimiters of one text, paired in a single pass.
//!
//! "Where does this opener close?" is asked once per call candidate by the invocation scanner. Counting
//! bytes forward from the opener makes each question cost the distance to the closer or, for an opener
//! that never closes, the rest of the file, so a file made of unclosed `a(` or `a<` (or of one deeply
//! nested call) is quadratic. One pass with a stack pairs every opener, and each question becomes a
//! binary search over the positions of the openers.

/// The positions of one kind of opening byte and the position of the byte that closes each.
pub(crate) struct DelimiterPairs {
    /// Positions of every opener, in increasing order.
    opens: Vec<usize>,
    /// For each opener in `opens`, the position of its closer, or `UNCLOSED`.
    closes: Vec<usize>,
}

const UNCLOSED: usize = usize::MAX;

impl DelimiterPairs {
    /// Pairs the `open_byte`s of `source` with the `close_byte`s that match them when only these two
    /// bytes are counted; every other byte, in particular any other kind of bracket, is ignored.
    pub(crate) fn new(source: &str, open_byte: u8, close_byte: u8) -> Self {
        let mut opens = Vec::new();
        let mut closes = Vec::new();
        // Indexes into `opens` of the openers that are still waiting for their closer.
        let mut waiting: Vec<usize> = Vec::new();
        for (at, byte) in source.bytes().enumerate() {
            if byte == open_byte {
                waiting.push(opens.len());
                opens.push(at);
                closes.push(UNCLOSED);
            } else if byte == close_byte
                && let Some(index) = waiting.pop()
            {
                closes[index] = at;
            }
        }
        Self { opens, closes }
    }

    /// The position of the byte that closes the opener at `open`.
    ///
    /// `None` when that opener is never closed and when `open` is not the position of an opener.
    pub(crate) fn closing(&self, open: usize) -> Option<usize> {
        let index = self.opens.binary_search(&open).ok()?;
        Some(self.closes[index]).filter(|&close| close != UNCLOSED)
    }
}

#[cfg(test)]
mod tests {
    use super::DelimiterPairs;

    /// The scan that the pairs replace: count the two bytes forward from the opener.
    fn counting_scan(source: &str, open: usize, open_byte: u8, close_byte: u8) -> Option<usize> {
        let mut depth = 0usize;
        for (offset, byte) in source.as_bytes()[open..].iter().copied().enumerate() {
            if byte == open_byte {
                depth += 1;
            } else if byte == close_byte {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
        }
        None
    }

    struct Rng(u64);

    impl Rng {
        fn roll(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn pairs_agree_with_the_counting_scan_on_random_text() {
        const ALPHABET: &[u8] = b"(()<>)<>a ({[]})=>->";
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for round in 0..2000 {
            let length = (rng.roll() % 40) as usize;
            let text: String = (0..length)
                .map(|_| char::from(ALPHABET[(rng.roll() % ALPHABET.len() as u64) as usize]))
                .collect();
            for (open_byte, close_byte) in [(b'(', b')'), (b'<', b'>'), (b'{', b'}')] {
                let pairs = DelimiterPairs::new(&text, open_byte, close_byte);
                for (at, byte) in text.bytes().enumerate() {
                    if byte == open_byte {
                        assert_eq!(
                            pairs.closing(at),
                            counting_scan(&text, at, open_byte, close_byte),
                            "round {round}: `{text}` at {at} for {}",
                            char::from(open_byte)
                        );
                    } else {
                        assert_eq!(pairs.closing(at), None, "`{text}` at {at} is no opener");
                    }
                }
            }
        }
    }

    #[test]
    fn unmatched_and_stray_delimiters_are_handled() {
        let text = ")) (a (b) c";
        let pairs = DelimiterPairs::new(text, b'(', b')');
        assert_eq!(pairs.closing(3), None, "the outer opener never closes");
        assert_eq!(pairs.closing(6), Some(8));
        assert_eq!(pairs.closing(0), None, "a closer is not an opener");
        assert_eq!(pairs.closing(100), None, "out of range");
    }

    #[test]
    fn a_deeply_nested_text_is_paired_without_recursion() {
        let depth = 200_000;
        let text = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
        let pairs = DelimiterPairs::new(&text, b'(', b')');
        assert_eq!(pairs.closing(0), Some(2 * depth - 1));
        assert_eq!(pairs.closing(depth - 1), Some(depth));
    }
}
