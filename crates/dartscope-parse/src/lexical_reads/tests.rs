use super::{assignment_operator_at, assignment_positions, precedes_assignment_in_statement};
use crate::source_structure::SourceStructure;

/// The scan that the positions and the structure replace, kept as the specification.
fn precedes_assignment_by_scanning(source: &str, start: usize) -> bool {
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
            b'{' if parens == 0 && brackets == 0 && braces == 0 => break,
            b'{' => braces += 1,
            b'}' if braces == 0 => break,
            b'}' => braces -= 1,
            b',' | b';' if parens == 0 && brackets == 0 && braces == 0 => break,
            _ => {}
        }
        if assignment_operator_at(bytes, at) {
            return true;
        }
        at += 1;
    }
    false
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
        "(", ")", "[", "]", "{", "}", ",", ";", "=", "==", "=>", "+=", "??=", "~/=", ">>=", "<=",
        "!=", "a", "b", " ", "\n", "+", "-", "<", ">",
    ];
    let count = rng.below(26);
    (0..count)
        .map(|_| PIECES[rng.below(PIECES.len())])
        .collect()
}

#[test]
fn the_precomputed_answer_is_what_the_scan_finds() {
    let mut rng = Rng(0x5EED_1234_ABCD_9876);
    for _ in 0..1500 {
        let source = random_source(&mut rng);
        let structure = SourceStructure::new(&source);
        let assignments = assignment_positions(source.as_bytes());
        for start in 0..=source.len() + 1 {
            assert_eq!(
                precedes_assignment_in_statement(&assignments, &structure, start),
                precedes_assignment_by_scanning(&source, start),
                "start {start} in {source:?}"
            );
        }
    }
}
