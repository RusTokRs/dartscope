//! TEMPORARY: compares `crate::uri` with the `uriparse` crate that it replaces, on damaged URIs.
//!
//! Run with `cargo test -p dartscope-resolve --lib -- --ignored --nocapture differential`. The test
//! prints every kind of difference with a few examples and fails only when a kind is on the list of
//! differences that were reviewed and accepted. It is removed together with the dependency.

use std::collections::BTreeMap;

use uriparse::{URI, URIReference};

use crate::uri::UriReference;

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

const PIECES: &[&str] = &[
    "file:", "http:", "package:", "FILE:", "x:", "1:", ":", "//", "///", "/", "./", "../", ".",
    "..", "a", "b", "lib", "A", "Z", "0", "9", "%20", "%2e", "%2E", "%2f", "%2F", "%41", "%7a",
    "%", "%4", "%g0", "%zz", "?", "#", "?q", "#f", "=", "&", ";", ",", "@", "u:p@", "[", "]",
    "[::1]", "[v1.x]", "[::g]", ":80", ":99999", "host", "h.ost-1_x~", " ", "\t", "\\", "^", "{",
    "}", "|", "`", "\"", "<", ">", "\u{e9}", "\u{0}", "~", "-", "_", "+", "*", "!", "$", "'", "(",
    ")", "C:", "C|", "..%2f", "%2e%2e/",
];

const BASES: &[&str] = &[
    "file:///__dartscope_project__/apps/demo/.dart_tool/package_config.json",
    "file:///cache/my%20package/",
    "http://a/b/c/d;p?q",
    "file:///C:/Users/demo/app/",
    "file://host/share/x",
    "mailto:a@b",
    "urn:x:y",
    "http://a",
    "FILE:///A/B/",
];

fn text(rng: &mut Rng) -> String {
    let count = 1 + rng.below(7);
    (0..count)
        .map(|_| PIECES[rng.below(PIECES.len())])
        .collect()
}

#[derive(Default)]
struct Report {
    counts: BTreeMap<&'static str, usize>,
    examples: BTreeMap<&'static str, Vec<String>>,
}

impl Report {
    fn note(&mut self, kind: &'static str, example: String) {
        *self.counts.entry(kind).or_default() += 1;
        let list = self.examples.entry(kind).or_default();
        if list.len() < 8 {
            list.push(example);
        }
    }
}

fn compare_parse(report: &mut Report, input: &str) {
    let mine = UriReference::parse(input);
    let theirs = URIReference::try_from(input);
    match (&mine, &theirs) {
        (Ok(_), Err(error)) => report.note(
            "parse: accepted here, rejected by uriparse",
            format!("{input:?} ({error})"),
        ),
        (Err(error), Ok(_)) => report.note(
            "parse: rejected here, accepted by uriparse",
            format!("{input:?} ({error:?})"),
        ),
        (Ok(mine), Ok(theirs)) => {
            let their_scheme = theirs.scheme().map(ToString::to_string);
            if mine.scheme().map(str::to_ascii_lowercase)
                != their_scheme.as_deref().map(str::to_ascii_lowercase)
            {
                report.note(
                    "components: scheme differs",
                    format!("{input:?}: {:?} / {their_scheme:?}", mine.scheme()),
                );
            }
            let their_authority = theirs.authority().map(ToString::to_string);
            if mine.authority().map(str::to_string) != their_authority {
                report.note(
                    "components: authority differs",
                    format!("{input:?}: {:?} / {their_authority:?}", mine.authority()),
                );
            }
            let their_query = theirs.query().map(ToString::to_string);
            if mine.query().map(str::to_string) != their_query {
                report.note(
                    "components: query differs",
                    format!("{input:?}: {:?} / {their_query:?}", mine.query()),
                );
            }
            let their_fragment = theirs.fragment().map(ToString::to_string);
            if mine.fragment().map(str::to_string) != their_fragment {
                report.note(
                    "components: fragment differs",
                    format!("{input:?}: {:?} / {their_fragment:?}", mine.fragment()),
                );
            }
            let (mine, theirs) = (mine.to_string(), theirs.to_string());
            if mine != theirs {
                if mine.eq_ignore_ascii_case(&theirs) {
                    report.note(
                        "print: differs only by case",
                        format!("{input:?}: {mine:?} / {theirs:?}"),
                    );
                } else {
                    report.note("print: differs", format!("{input:?}: {mine:?} / {theirs:?}"));
                }
            }
        }
        (Err(_), Err(_)) => {}
    }
}

fn compare_resolve(report: &mut Report, base: &str, reference: &str) {
    let (Ok(my_base), Ok(their_base)) = (UriReference::parse_absolute(base), URI::try_from(base))
    else {
        return;
    };
    let (Ok(my_reference), Ok(their_reference)) =
        (UriReference::parse(reference), URIReference::try_from(reference))
    else {
        return;
    };
    let mine = my_base.resolve(&my_reference).to_string();
    let theirs = their_base.resolve(&their_reference).to_string();
    if mine != theirs {
        if mine.eq_ignore_ascii_case(&theirs) {
            report.note(
                "resolve: differs only by case",
                format!("{base:?} + {reference:?}: {mine:?} / {theirs:?}"),
            );
        } else {
            report.note(
                "resolve: differs",
                format!("{base:?} + {reference:?}: {mine:?} / {theirs:?}"),
            );
        }
    }
}

#[test]
#[ignore = "a one-off comparison with the crate that this module replaces"]
fn differential_against_uriparse() {
    let mut rng = Rng(0x00C0_FFEE_D00D_F00D);
    let mut report = Report::default();
    let mut compared = 0usize;
    for _ in 0..300_000 {
        let input = text(&mut rng);
        compare_parse(&mut report, &input);
        compared += 1;
        let base = if rng.below(3) == 0 {
            format!("file:///{}", text(&mut rng))
        } else {
            BASES[rng.below(BASES.len())].to_string()
        };
        compare_resolve(&mut report, &base, &input);
    }
    // The examples of RFC 3986 section 5.4 and the references of the package-configuration tests.
    for reference in [
        "g:h", "g", "./g", "g/", "/g", "//g", "?y", "g?y", "#s", "g#s", "g?y#s", ";x", "g;x",
        "g;x?y#s", "", ".", "./", "..", "../", "../g", "../..", "../../", "../../g", "../../../g",
        "/./g", "/../g", "g.", ".g", "g..", "..g", "./../g", "./g/.", "g/./h", "g/../h",
        "g;x=1/./y", "g;x=1/../y", "g?y/./x", "g?y/../x", "g#s/./x", "g#s/../x", "http:g",
        "lib/", "lib", "../../../packages/shared", "%2e%2e/outside/", "lib%20src/",
        "file:///cache/%70kg/",
    ] {
        compare_parse(&mut report, reference);
        for base in BASES {
            compare_resolve(&mut report, base, reference);
        }
    }

    println!();
    println!("differential: {compared} random inputs compared");
    if report.counts.is_empty() {
        println!("differential: no difference");
    }
    for (kind, count) in &report.counts {
        println!("differential [{kind}] x{count}");
        for example in &report.examples[kind] {
            println!("differential     {example}");
        }
    }
}
