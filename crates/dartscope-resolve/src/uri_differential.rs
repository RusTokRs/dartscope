//! TEMPORARY: compares `crate::uri` with the `uriparse` crate that it replaces, on damaged URIs.
//!
//! Run with `cargo test -p dartscope-resolve --lib -- --ignored --nocapture differential`. Every
//! difference is either one that was reviewed and is listed in `known` below, or it is reported as
//! unexplained and fails the test. The module is removed together with the dependency.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use uriparse::{URI, URIReference};

use crate::uri::{UriError, UriReference};

static LAST_PANIC: Mutex<String> = Mutex::new(String::new());

/// Runs `call` and turns a panic into an `Err` that carries the location and the message.
fn guarded<T>(call: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(call)).map_err(|_| {
        LAST_PANIC
            .lock()
            .map(|text| text.replace('\n', " | "))
            .unwrap_or_default()
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

const PIECES: &[&str] = &[
    "file:",
    "http:",
    "package:",
    "FILE:",
    "x:",
    "1:",
    ":",
    "//",
    "///",
    "/",
    "./",
    "../",
    ".",
    "..",
    "a",
    "b",
    "lib",
    "A",
    "Z",
    "0",
    "9",
    "%20",
    "%2e",
    "%2E",
    "%2f",
    "%2F",
    "%41",
    "%7a",
    "%",
    "%4",
    "%g0",
    "%zz",
    "?",
    "#",
    "?q",
    "#f",
    "=",
    "&",
    ";",
    ",",
    "@",
    "u:p@",
    "[",
    "]",
    "[::1]",
    "[v1.x]",
    "[::g]",
    ":80",
    ":99999",
    "host",
    "h.ost-1_x~",
    " ",
    "\t",
    "\\",
    "^",
    "{",
    "}",
    "|",
    "`",
    "\"",
    "<",
    ">",
    "\u{e9}",
    "\u{0}",
    "~",
    "-",
    "_",
    "+",
    "*",
    "!",
    "$",
    "'",
    "(",
    ")",
    "C:",
    "C|",
    "..%2f",
    "%2e%2e/",
    ".%2e/",
    "%2e./",
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

/// Differences that were reviewed: what each is, and why this module is right.
const KNOWN: &[(&str, &str)] = &[
    (
        "uriparse panics",
        "its `resolve` unwraps an error when the result would start with `//` without an authority; here it is `UriError::Path`",
    ),
    (
        "uriparse rejects: port overflow",
        "RFC 3986 allows any run of digits as the port",
    ),
    (
        "uriparse rejects: colon in the first segment of an absolute path",
        "only a relative path without a leading `/` has to avoid a colon there",
    ),
    (
        "authority: an empty port is kept",
        "`host:` is valid and is printed as it was written",
    ),
    (
        "print: uriparse drops an empty port and writes an empty path as `/`",
        "this module prints what it was given; RFC 5.2.2 keeps an empty path empty",
    ),
    (
        "print: uriparse normalizes the host case and the form of escapes",
        "RFC 3986 section 6.2.2 makes them equivalent; this module keeps the text as written",
    ),
];

#[derive(Default)]
struct Report {
    known: BTreeMap<&'static str, usize>,
    unexplained: BTreeMap<String, usize>,
    examples: BTreeMap<String, Vec<String>>,
}

impl Report {
    fn known(&mut self, kind: &'static str) {
        assert!(KNOWN.iter().any(|(name, _)| *name == kind), "{kind}");
        *self.known.entry(kind).or_default() += 1;
    }

    fn unexplained(&mut self, kind: impl Into<String>, example: String) {
        let kind = kind.into();
        *self.unexplained.entry(kind.clone()).or_default() += 1;
        let list = self.examples.entry(kind).or_default();
        if list.len() < 400 {
            list.push(example);
        }
    }
}

/// What `uriparse` prints for the same components: an empty path next to an authority becomes `/`.
fn as_uriparse_prints(reference: &UriReference) -> String {
    let mut printed = String::new();
    if let Some(scheme) = reference.scheme() {
        printed.push_str(scheme);
        printed.push(':');
    }
    if let Some(authority) = reference.authority() {
        printed.push_str("//");
        printed.push_str(authority.strip_suffix(':').unwrap_or(authority));
    }
    if reference.authority().is_some() && reference.path().is_empty() {
        printed.push('/');
    }
    printed.push_str(reference.path());
    if let Some(query) = reference.query() {
        printed.push('?');
        printed.push_str(query);
    }
    if let Some(fragment) = reference.fragment() {
        printed.push('#');
        printed.push_str(fragment);
    }
    printed
}

/// Decodes escapes of unreserved characters and upper-cases the hex digits of the others.
fn normalize_escapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escape = bytes
            .get(index + 1..index + 3)
            .filter(|_| bytes[index] == b'%')
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match escape {
            Some(value)
                if value.is_ascii_alphanumeric() || matches!(value, b'-' | b'.' | b'_' | b'~') =>
            {
                output.push(value);
                index += 3;
            }
            Some(value) => {
                output.extend_from_slice(format!("%{value:02X}").as_bytes());
                index += 3;
            }
            None => {
                output.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8(output).unwrap_or_default()
}

/// The components of a printed URI, compared up to the case of the host and the form of escapes.
#[derive(PartialEq)]
struct Form {
    scheme: Option<String>,
    authority: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

fn form(reference: &UriReference) -> Form {
    let authority = reference.authority().map(|authority| {
        authority
            .strip_suffix(':')
            .unwrap_or(authority)
            .to_ascii_lowercase()
    });
    let mut path = normalize_escapes(reference.path());
    // An empty path next to an authority is the same as `/`.
    if authority.is_some() && path.is_empty() {
        path = "/".to_string();
    }
    Form {
        scheme: reference.scheme().map(str::to_string),
        authority,
        path,
        query: reference.query().map(normalize_escapes),
        fragment: reference.fragment().map(normalize_escapes),
    }
}

/// Whether two printed URIs are the same up to the case of the host and the form of escapes.
fn equivalent(left: &str, right: &str) -> bool {
    match (UriReference::parse(left), UriReference::parse(right)) {
        (Ok(left), Ok(right)) => form(&left) == form(&right),
        _ => false,
    }
}

/// Judges what this module printed against what `uriparse` printed for the same input.
fn judge_print(
    report: &mut Report,
    kind: &str,
    mine: &UriReference,
    theirs: &str,
    example: String,
) {
    let expected = as_uriparse_prints(mine);
    let printed = mine.to_string();
    if printed == theirs {
        return;
    }
    if expected == theirs {
        report.known("print: uriparse drops an empty port and writes an empty path as `/`");
    } else if equivalent(&printed, theirs) {
        report.known("print: uriparse normalizes the host case and the form of escapes");
    } else {
        report.unexplained(kind, format!("{example}: {printed:?} / {theirs:?}"));
    }
}

fn compare_parse(report: &mut Report, input: &str) {
    let mine = UriReference::parse(input);
    let theirs = match guarded(|| URIReference::try_from(input)) {
        Ok(theirs) => theirs,
        Err(panic) => {
            report.unexplained("uriparse panics in try_from", format!("{input:?}: {panic}"));
            return;
        }
    };
    match (&mine, &theirs) {
        (Ok(_), Err(error)) => {
            let reason = error.to_string();
            let absolute_path = input
                .split(['?', '#'])
                .next()
                .is_some_and(|path| path.starts_with('/') && !path.starts_with("//"));
            if reason.contains("port overflow") {
                report.known("uriparse rejects: port overflow");
            } else if reason.contains("colon segment") && absolute_path {
                report.known("uriparse rejects: colon in the first segment of an absolute path");
            } else {
                report.unexplained(
                    format!("parse: accepted here, rejected by uriparse ({reason})"),
                    format!("{input:?}"),
                );
            }
        }
        (Err(error), Ok(_)) => report.unexplained(
            format!("parse: rejected here ({error:?}), accepted by uriparse"),
            format!("{input:?}"),
        ),
        (Ok(mine), Ok(theirs)) => {
            let their_scheme = theirs.scheme().map(ToString::to_string);
            if mine.scheme().map(str::to_string) != their_scheme.map(|s| s.to_ascii_lowercase()) {
                report.unexplained(
                    "components: scheme differs",
                    format!("{input:?}: {:?} / {:?}", mine.scheme(), theirs.scheme()),
                );
            }
            let their_authority = theirs.authority().map(ToString::to_string);
            if mine.authority().map(str::to_string) != their_authority {
                let only_the_port = mine
                    .authority()
                    .and_then(|authority| authority.strip_suffix(':'))
                    .map(str::to_string)
                    == their_authority;
                if only_the_port {
                    report.known("authority: an empty port is kept");
                } else {
                    report.unexplained(
                        "components: authority differs",
                        format!("{input:?}: {:?} / {their_authority:?}", mine.authority()),
                    );
                }
            }
            let their_query = theirs.query().map(ToString::to_string);
            if mine.query().map(str::to_string) != their_query {
                report.unexplained(
                    "components: query differs",
                    format!("{input:?}: {:?} / {their_query:?}", mine.query()),
                );
            }
            let their_fragment = theirs.fragment().map(ToString::to_string);
            if mine.fragment().map(str::to_string) != their_fragment {
                report.unexplained(
                    "components: fragment differs",
                    format!("{input:?}: {:?} / {their_fragment:?}", mine.fragment()),
                );
            }
            judge_print(
                report,
                "print: differs",
                mine,
                &theirs.to_string(),
                format!("{input:?}"),
            );
        }
        (Err(_), Err(_)) => {}
    }
}

fn compare_resolve(report: &mut Report, base: &str, reference: &str) {
    let their_base = match guarded(|| URI::try_from(base)) {
        Ok(result) => result,
        Err(panic) => {
            report.unexplained(
                "uriparse panics in URI::try_from",
                format!("{base:?}: {panic}"),
            );
            return;
        }
    };
    let their_reference = match guarded(|| URIReference::try_from(reference)) {
        Ok(result) => result,
        Err(panic) => {
            report.unexplained(
                "uriparse panics in URIReference::try_from",
                format!("{reference:?}: {panic}"),
            );
            return;
        }
    };
    let (Ok(my_base), Ok(their_base)) = (UriReference::parse_absolute(base), their_base) else {
        return;
    };
    let (Ok(my_reference), Ok(their_reference)) = (UriReference::parse(reference), their_reference)
    else {
        return;
    };
    let mine = my_base.resolve(&my_reference);
    let theirs = guarded(|| their_base.resolve(&their_reference).to_string());
    match (mine, theirs) {
        (Err(UriError::Path), Err(_)) => report.known("uriparse panics"),
        (Err(error), Err(panic)) => report.unexplained(
            "resolve: both fail, differently",
            format!("{base:?} + {reference:?}: {error:?} / {panic}"),
        ),
        (Ok(mine), Err(panic)) => report.unexplained(
            "resolve: uriparse panics, this module resolves",
            format!("{base:?} + {reference:?}: {mine} / {panic}"),
        ),
        (Err(error), Ok(theirs)) => report.unexplained(
            "resolve: this module fails, uriparse resolves",
            format!("{base:?} + {reference:?}: {error:?} / {theirs:?}"),
        ),
        (Ok(mine), Ok(theirs)) => judge_print(
            report,
            "resolve: differs",
            &mine,
            &theirs,
            format!("{base:?} + {reference:?}"),
        ),
    }
}

#[test]
#[ignore = "a one-off comparison with the crate that this module replaces"]
fn differential_against_uriparse() {
    std::panic::set_hook(Box::new(|info| {
        if let Ok(mut last) = LAST_PANIC.lock() {
            *last = info.to_string();
        }
    }));
    let mut rng = Rng(0x00C0_FFEE_D00D_F00D);
    let mut report = Report::default();
    let mut compared = 0usize;
    for _ in 0..400_000 {
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
        "g:h",
        "g",
        "./g",
        "g/",
        "/g",
        "//g",
        "?y",
        "g?y",
        "#s",
        "g#s",
        "g?y#s",
        ";x",
        "g;x",
        "g;x?y#s",
        "",
        ".",
        "./",
        "..",
        "../",
        "../g",
        "../..",
        "../../",
        "../../g",
        "../../../g",
        "/./g",
        "/../g",
        "g.",
        ".g",
        "g..",
        "..g",
        "./../g",
        "./g/.",
        "g/./h",
        "g/../h",
        "g;x=1/./y",
        "g;x=1/../y",
        "g?y/./x",
        "g?y/../x",
        "g#s/./x",
        "g#s/../x",
        "http:g",
        "lib/",
        "lib",
        "../../../packages/shared",
        "%2e%2e/outside/",
        "lib%20src/",
        "file:///cache/%70kg/",
    ] {
        compare_parse(&mut report, reference);
        for base in BASES {
            compare_resolve(&mut report, base, reference);
        }
    }

    println!();
    println!("differential: {compared} random inputs compared");
    for (kind, count) in &report.known {
        println!("differential known [{kind}] x{count}");
    }
    for (kind, count) in &report.unexplained {
        println!("differential UNEXPLAINED [{kind}] x{count}");
        let examples = &report.examples[kind];
        let step = (examples.len() / 10).max(1);
        for example in examples.iter().step_by(step).take(10) {
            println!("differential     {example}");
        }
    }
    assert!(
        report.unexplained.is_empty(),
        "unexplained differences: {:?}",
        report.unexplained.keys().collect::<Vec<_>>()
    );
}
