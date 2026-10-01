use dartscope_core::{DartFileAnalysis, DartFileInput};

use super::CallableHeaders;
use crate::declaration_tables::supports_parameters;

/// The scan that `CallableHeaders` replaces, kept as the specification.
fn modeled_by_scanning(
    analysis: &DartFileAnalysis,
    source: &str,
    parameter_start: usize,
    body_start: usize,
) -> bool {
    analysis.declarations.iter().any(|declaration| {
        supports_parameters(declaration.kind)
            && declaration.declaration_span.as_ref().is_some_and(|span| {
                span.byte_start <= parameter_start
                    && body_start < span.byte_end
                    && !source[span.byte_start..parameter_start].contains('{')
                    && !source[span.byte_start..parameter_start].contains("=>")
            })
    })
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

const SOURCE: &str = "class A {
  int f(int a) => a + 1;
  void g(int b) { var c = (int x) => x + b; h((y) { return y; }); }
  A(this.v) : w = v => 1;
  int get z => 3;
  set z(int q) { _z = q; }
  int operator +(int o) => v + o;
}
int top(int a, [int b = 2]) { return a; }
int arrow(int a) => (a) => a;
";

#[test]
fn a_header_is_recognized_exactly_where_the_scan_recognized_it() {
    let analysis = crate::analyze_file(DartFileInput::new("lib/a.dart", SOURCE.to_string()));
    assert!(analysis.declarations.len() >= 8);
    let len = SOURCE.len();

    // The queries the region collectors make: every opening parenthesis with the next `{` or `=>`.
    let bytes = SOURCE.as_bytes();
    let headers = CallableHeaders::new(&analysis, SOURCE);
    for open in (0..len).filter(|&at| bytes[at] == b'(') {
        for body in open + 1..len {
            assert_eq!(
                headers.models(open, body),
                modeled_by_scanning(&analysis, SOURCE, open, body),
                "real spans, parenthesis {open}, body {body}"
            );
        }
    }

    // The same declarations with spans that overlap in ways real code does not.
    let mut rng = Rng(0x0DDB_A11C_0FFE_E5ED);
    for _ in 0..40 {
        let mut shuffled = analysis.clone();
        for declaration in &mut shuffled.declarations {
            if let Some(span) = declaration.declaration_span.as_mut() {
                span.byte_start = rng.below(len);
                span.byte_end = span.byte_start + rng.below(len);
            }
        }
        let headers = CallableHeaders::new(&shuffled, SOURCE);
        for _ in 0..300 {
            let open = rng.below(len);
            let body = open + 1 + rng.below(len - open);
            assert_eq!(
                headers.models(open, body),
                modeled_by_scanning(&shuffled, SOURCE, open, body),
                "shuffled spans, parenthesis {open}, body {body}"
            );
        }
    }
}
