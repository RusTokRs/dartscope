use crate::delimiter_pairs::DelimiterPairs;
use crate::identifiers::{is_identifier_continue, is_identifier_start};

/// Where the openers of a text close, as far as the invocation scan needs to know.
///
/// Every answer is the position of the first closer at which the count of the two bytes returns to
/// zero when counting forward from the opener, or `None` when the text ends first.
pub(super) trait Closers {
    fn closing_paren(&self, open: usize) -> Option<usize>;
    fn closing_angle(&self, open: usize) -> Option<usize>;
    fn closing_brace(&self, open: usize) -> Option<usize>;
}

/// The delimiter pairs of one masked source, found in one pass per kind of bracket.
///
/// The scan asks about every `(` that follows an identifier and every `<` that may open type
/// arguments. Counting forward for each of them is quadratic for unclosed or deeply nested input.
pub(super) struct Delimiters {
    parens: DelimiterPairs,
    angles: DelimiterPairs,
    braces: DelimiterPairs,
}

impl Delimiters {
    pub(super) fn new(masked_source: &str) -> Self {
        Self {
            parens: DelimiterPairs::new(masked_source, b'(', b')'),
            angles: DelimiterPairs::new(masked_source, b'<', b'>'),
            braces: DelimiterPairs::new(masked_source, b'{', b'}'),
        }
    }
}

impl Closers for Delimiters {
    fn closing_paren(&self, open: usize) -> Option<usize> {
        self.parens.closing(open)
    }

    fn closing_angle(&self, open: usize) -> Option<usize> {
        self.angles.closing(open)
    }

    fn closing_brace(&self, open: usize) -> Option<usize> {
        self.braces.closing(open)
    }
}

/// How much source text the facts of one file may copy, per byte of the file.
///
/// A call chain copies the dotted prefix into the target of every call in it, and a call nested in
/// other calls is copied into the arguments of each of them, so the text that the facts hold grows
/// with the square of the length of the chain or the depth of the nesting: 64 KiB of nested calls
/// would produce 700 MB of facts. Real code stays far below the factor (the deepest widget trees
/// copy each byte about ten times); input that exceeds it is cut off with a diagnostic.
const BUDGET_PER_SOURCE_BYTE: usize = 32;

/// The part of the budget that does not depend on the size of the file.
const BUDGET_BASE_BYTES: usize = 1 << 20;

/// Bytes of source text that facts may still copy; one budget each for targets and for arguments.
#[derive(Debug)]
pub(super) struct CopyBudget {
    remaining: usize,
}

impl CopyBudget {
    pub(super) fn for_source(source_len: usize) -> Self {
        Self {
            remaining: source_len
                .saturating_mul(BUDGET_PER_SOURCE_BYTE)
                .saturating_add(BUDGET_BASE_BYTES),
        }
    }

    /// Takes `bytes` out of the budget; `false`, with the budget emptied, when they do not fit.
    pub(super) fn spend(&mut self, bytes: usize) -> bool {
        match self.remaining.checked_sub(bytes) {
            Some(remaining) => {
                self.remaining = remaining;
                true
            }
            None => {
                self.remaining = 0;
                false
            }
        }
    }
}

/// The call candidates of a text, in the order of their chains.
pub(super) struct Scan {
    pub(super) candidates: Vec<CallCandidate>,
    /// The start of the chain whose targets did not fit the budget; no later chain was scanned.
    pub(super) stopped_at: Option<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct CallCandidate {
    pub(super) target: String,
    pub(super) start: usize,
    pub(super) open: usize,
    pub(super) close: usize,
    pub(super) end: usize,
    pub(super) result_members: Vec<String>,
}

pub(super) fn scan_call_candidates(
    source: &str,
    closers: &impl Closers,
    budget: &mut CopyBudget,
) -> Scan {
    let bytes = source.as_bytes();
    let mut candidates = Vec::new();
    let mut stopped_at = None;
    let mut index = 0usize;
    while index < bytes.len() {
        if is_identifier_start(bytes[index]) && is_chain_start(bytes, index) {
            let (calls, exhausted) = scan_chain(source, index, closers, budget);
            candidates.extend(calls);
            if exhausted {
                stopped_at = Some(index);
                break;
            }
            index = identifier_end(bytes, index);
        } else {
            index += 1;
        }
    }
    Scan {
        candidates,
        stopped_at,
    }
}

/// The calls of the chain that starts at `start`, and whether the budget ran out inside it.
fn scan_chain(
    source: &str,
    start: usize,
    closers: &impl Closers,
    budget: &mut CopyBudget,
) -> (Vec<CallCandidate>, bool) {
    let bytes = source.as_bytes();
    let first_end = identifier_end(bytes, start);
    let first = &source[start..first_end];
    if is_reserved_target(first) {
        return (Vec::new(), false);
    }

    let mut parts = vec![first.to_string()];
    let mut cursor = first_end;
    let mut calls: Vec<CallCandidate> = Vec::new();

    loop {
        cursor = skip_whitespace(bytes, cursor);
        cursor = skip_type_arguments(source, cursor, closers).unwrap_or(cursor);
        cursor = skip_whitespace(bytes, cursor);

        if bytes.get(cursor) == Some(&b'(') {
            let Some(close) = closers.closing_paren(cursor) else {
                break;
            };
            let target = parts.join(".");
            if !budget.spend(target.len()) {
                return (calls, true);
            }
            calls.push(CallCandidate {
                target,
                start,
                open: cursor,
                close,
                end: close + 1,
                result_members: Vec::new(),
            });
            cursor = close + 1;
            continue;
        }

        cursor = skip_postfix_nullability(bytes, cursor);
        cursor = skip_whitespace(bytes, cursor);
        if bytes.get(cursor) != Some(&b'.') {
            break;
        }
        cursor = skip_whitespace(bytes, cursor + 1);
        if !bytes
            .get(cursor)
            .is_some_and(|byte| is_identifier_start(*byte))
        {
            break;
        }
        let member_end = identifier_end(bytes, cursor);
        let member = source[cursor..member_end].to_string();
        parts.push(member.clone());
        cursor = skip_whitespace(bytes, member_end);
        let after_type_arguments = skip_type_arguments(source, cursor, closers).unwrap_or(cursor);
        let after = skip_whitespace(bytes, after_type_arguments);
        if bytes.get(after) != Some(&b'(')
            && let Some(call) = calls.last_mut()
        {
            call.result_members.push(member);
            call.end = member_end;
        }
        cursor = after_type_arguments;
    }

    (calls, false)
}

fn skip_type_arguments(source: &str, at: usize, closers: &impl Closers) -> Option<usize> {
    if source.as_bytes().get(at) != Some(&b'<') {
        return None;
    }
    let close = closers.closing_angle(at)?;
    let after = skip_whitespace(source.as_bytes(), close + 1);
    (source.as_bytes().get(after) == Some(&b'(')).then_some(close + 1)
}

fn skip_postfix_nullability(bytes: &[u8], mut at: usize) -> usize {
    loop {
        at = skip_whitespace(bytes, at);
        match bytes.get(at) {
            Some(b'!') => at += 1,
            Some(b'?') if bytes.get(at + 1) == Some(&b'.') => at += 1,
            _ => return at,
        }
    }
}

fn is_chain_start(bytes: &[u8], at: usize) -> bool {
    if at == 0 {
        return true;
    }
    !matches!(bytes[at - 1], b'.') && !is_identifier_continue(bytes[at - 1])
}

fn identifier_end(bytes: &[u8], mut at: usize) -> usize {
    while bytes
        .get(at)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        at += 1;
    }
    at
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

fn is_reserved_target(target: &str) -> bool {
    matches!(
        target,
        "assert"
            | "catch"
            | "class"
            | "do"
            | "else"
            | "enum"
            | "extension"
            | "for"
            | "if"
            | "mixin"
            | "return"
            | "switch"
            | "typedef"
            | "while"
            | "with"
    )
}

#[cfg(test)]
mod tests {
    use super::super::arguments::invocation_arguments;
    use super::{CallCandidate, Closers, CopyBudget, Delimiters, scan_call_candidates};

    /// The candidates of `source` with a budget that these small texts never exhaust.
    fn candidates(source: &str, closers: &impl Closers) -> Vec<CallCandidate> {
        let mut budget = CopyBudget::for_source(source.len());
        let scan = scan_call_candidates(source, closers, &mut budget);
        assert_eq!(scan.stopped_at, None);
        scan.candidates
    }

    /// The counting scan that `Delimiters` replaces: forward from the opener, one byte at a time.
    struct Counting<'a>(&'a str);

    impl Counting<'_> {
        fn close(&self, open: usize, open_byte: u8, close_byte: u8) -> Option<usize> {
            let mut depth = 0usize;
            for (offset, byte) in self.0.as_bytes()[open..].iter().copied().enumerate() {
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
    }

    impl Closers for Counting<'_> {
        fn closing_paren(&self, open: usize) -> Option<usize> {
            self.close(open, b'(', b')')
        }

        fn closing_angle(&self, open: usize) -> Option<usize> {
            self.close(open, b'<', b'>')
        }

        fn closing_brace(&self, open: usize) -> Option<usize> {
            self.close(open, b'{', b'}')
        }
    }

    #[test]
    fn scans_chained_and_result_member_calls() {
        let source = "DefaultAssetBundle.of(context).loadString(          ); AppLocalizations.of(context)!.welcomeMessage";
        let calls = candidates(source, &Delimiters::new(source));
        assert!(
            calls
                .iter()
                .any(|call| call.target == "DefaultAssetBundle.of")
        );
        assert!(
            calls
                .iter()
                .any(|call| call.target == "DefaultAssetBundle.of.loadString")
        );
        let localization = calls
            .iter()
            .find(|call| call.target == "AppLocalizations.of")
            .unwrap();
        assert_eq!(localization.result_members, ["welcomeMessage"]);
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn paired_delimiters_give_the_candidates_and_arguments_of_counting_forward() {
        const TOKENS: &[&str] = &[
            "a", "b", "f", "g", ".", ".", "(", "(", ")", ")", "<", ">", ",", " ", "!", "?.", "{",
            "}", "[", "]", ";", "=>", "'x'", ":", "assert", "if", "key: ", "\n", "a(", "f(",
            "g.b(", "<a>", "{k: ",
        ];
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let mut candidates_seen = 0usize;
        for round in 0..6000 {
            let count = 1 + (rng.next() % 60) as usize;
            let text: String = (0..count)
                .map(|_| TOKENS[(rng.next() % TOKENS.len() as u64) as usize])
                .collect();
            let paired = Delimiters::new(&text);
            let counting = Counting(&text);
            let from_pairs = candidates(&text, &paired);
            let from_counting = candidates(&text, &counting);
            assert_eq!(
                format!("{from_pairs:?}"),
                format!("{from_counting:?}"),
                "round {round}: candidates of `{text}`"
            );
            for candidate in &from_pairs {
                let with_pairs = invocation_arguments(
                    &text,
                    &text,
                    candidate.open + 1,
                    candidate.close,
                    &paired,
                );
                let with_counting = invocation_arguments(
                    &text,
                    &text,
                    candidate.open + 1,
                    candidate.close,
                    &counting,
                );
                assert_eq!(
                    format!("{with_pairs:?}"),
                    format!("{with_counting:?}"),
                    "round {round}: arguments of `{}` in `{text}`",
                    candidate.target
                );
            }
            candidates_seen += from_pairs.len();
        }
        assert!(candidates_seen > 500, "{candidates_seen} candidates");
    }

    #[test]
    fn type_arguments_before_a_call_are_skipped_only_when_a_call_follows() {
        let source = "a<b>(c); d<e>; f < g > (h); i<j<k>>(l); m<n>> (o)";
        let targets: Vec<String> = candidates(source, &Delimiters::new(source))
            .into_iter()
            .map(|call| call.target)
            .collect();
        // `m<n>> (o)` closes its angle after `n`; a `>` follows, not a call.
        assert_eq!(targets, ["a", "f", "i"]);
    }

    #[test]
    fn unclosed_openers_are_answered_without_scanning_to_the_end_of_the_file() {
        // Counting forward from each of these openers reads the rest of the file: 200,000 openers
        // over 600,000 bytes are about 6 * 10^10 byte reads, minutes of work instead of milliseconds.
        let calls = "a(".repeat(200_000);
        assert!(candidates(&calls, &Delimiters::new(&calls)).is_empty());
        let angles = "a<".repeat(200_000);
        assert!(candidates(&angles, &Delimiters::new(&angles)).is_empty());
        let nested_angles = format!("{}{}", "a<".repeat(100_000), ">".repeat(100_000));
        assert!(candidates(&nested_angles, &Delimiters::new(&nested_angles)).is_empty());
    }

    #[test]
    fn the_budget_takes_what_fits_and_refuses_the_rest() {
        let mut budget = CopyBudget::for_source(0);
        assert!(
            budget.spend(1 << 20),
            "the base amount is available to an empty file"
        );
        assert!(!budget.spend(1), "nothing is left");
        let mut budget = CopyBudget::for_source(10);
        assert!(budget.spend(320 + (1 << 20)));
        assert!(!budget.spend(1));
        let mut budget = CopyBudget::for_source(10);
        assert!(
            !budget.spend(321 + (1 << 20)),
            "a request that does not fit is refused"
        );
    }

    #[test]
    fn a_chain_whose_targets_outgrow_the_budget_stops_the_scan() {
        // Every call of the chain copies the dotted prefix before it: 30,000 calls would be about
        // 900 MB of targets out of 150 KB of source.
        let source = format!("a{}()", "().b".repeat(30_000));
        let delimiters = Delimiters::new(&source);
        let mut budget = CopyBudget::for_source(source.len());
        let scan = scan_call_candidates(&source, &delimiters, &mut budget);
        assert_eq!(scan.stopped_at, Some(0));
        assert!(
            (1000..30_000).contains(&scan.candidates.len()),
            "{} candidates",
            scan.candidates.len()
        );
        let copied: usize = scan.candidates.iter().map(|call| call.target.len()).sum();
        assert!(
            copied <= 32 * source.len() + (1 << 20),
            "{copied} bytes of targets"
        );
        assert_eq!(scan.candidates[0].target, "a");
        assert_eq!(scan.candidates[1].target, "a.b");
    }
}
