//! A small RFC 3986 implementation: parse a URI reference, resolve it against a base, print it.
//!
//! `dartscope-resolve` decides which paths `package:` URIs and the `rootUri` and `packageUri` values
//! of `package_config.json` may name, so the code that reads those URIs is part of the security
//! boundary of path resolution. It used to be the `uriparse` crate, whose last release is from 2022;
//! this module covers exactly what the resolver needs (syntax validation of a reference, the
//! reference resolution of RFC 3986 section 5.2 and its inverse, printing) in code that this
//! repository owns, tests against the examples of RFC 3986 section 5.4, and fuzzes.
//!
//! Syntax is validated, not normalized: percent-escapes keep their case, an escaped dot is not a dot
//! segment, and the scheme and host keep the case they were written in. Callers that compare
//! URIs decode and lower-case what they compare.

use std::fmt;
use std::net::Ipv6Addr;
use std::str::FromStr;

/// Why a string is not a URI reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UriError {
    /// A `%` that is not followed by two hexadecimal digits.
    Escape,
    /// A character that its component does not allow, or a scheme-less first segment with a `:`.
    Character,
    /// The authority is not `[userinfo@]host[:port]`.
    Authority,
    /// A URI (as opposed to a reference) without a scheme.
    MissingScheme,
}

/// A URI reference: a URI, or a relative reference that is resolved against one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UriReference {
    scheme: Option<String>,
    authority: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

impl UriReference {
    /// Parses a URI reference (`URI-reference` of RFC 3986): absolute or relative.
    pub(crate) fn parse(input: &str) -> Result<Self, UriError> {
        let (without_fragment, fragment) = split_at_first(input, '#');
        let (without_query, query) = split_at_first(without_fragment, '?');
        let (scheme, hierarchical) = split_scheme(without_query);
        let (authority, path) = match hierarchical.strip_prefix("//") {
            Some(after) => {
                let end = after.find('/').unwrap_or(after.len());
                (Some(&after[..end]), &after[end..])
            }
            None => (None, hierarchical),
        };

        if let Some(authority) = authority {
            validate_authority(authority)?;
        }
        validate(path, |byte| is_pchar(byte) || byte == b'/')?;
        // The first segment of a relative path would read as a scheme if it held a `:`.
        if scheme.is_none()
            && authority.is_none()
            && path.split('/').next().is_some_and(|segment| segment.contains(':'))
        {
            return Err(UriError::Character);
        }
        if let Some(query) = query {
            validate(query, |byte| is_pchar(byte) || matches!(byte, b'/' | b'?'))?;
        }
        if let Some(fragment) = fragment {
            validate(fragment, |byte| is_pchar(byte) || matches!(byte, b'/' | b'?'))?;
        }
        Ok(Self {
            scheme: scheme.map(str::to_string),
            authority: authority.map(str::to_string),
            path: path.to_string(),
            query: query.map(str::to_string),
            fragment: fragment.map(str::to_string),
        })
    }

    /// Parses a URI: a reference that has a scheme.
    pub(crate) fn parse_absolute(input: &str) -> Result<Self, UriError> {
        let reference = Self::parse(input)?;
        if reference.scheme.is_none() {
            return Err(UriError::MissingScheme);
        }
        Ok(reference)
    }

    pub(crate) fn scheme(&self) -> Option<&str> {
        self.scheme.as_deref()
    }

    pub(crate) fn authority(&self) -> Option<&str> {
        self.authority.as_deref()
    }

    pub(crate) fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub(crate) fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// Resolves `reference` against this URI, the base, as RFC 3986 section 5.2.2 does (strict:
    /// a reference with a scheme is never read as relative to the base).
    pub(crate) fn resolve(&self, reference: &UriReference) -> UriReference {
        let mut target = UriReference {
            scheme: None,
            authority: None,
            path: String::new(),
            query: None,
            fragment: reference.fragment.clone(),
        };
        if reference.scheme.is_some() {
            target.scheme.clone_from(&reference.scheme);
            target.authority.clone_from(&reference.authority);
            target.path = remove_dot_segments(&reference.path);
            target.query.clone_from(&reference.query);
        } else {
            if reference.authority.is_some() {
                target.authority.clone_from(&reference.authority);
                target.path = remove_dot_segments(&reference.path);
                target.query.clone_from(&reference.query);
            } else {
                if reference.path.is_empty() {
                    target.path.clone_from(&self.path);
                    target.query = reference
                        .query
                        .clone()
                        .or_else(|| self.query.clone());
                } else {
                    if reference.path.starts_with('/') {
                        target.path = remove_dot_segments(&reference.path);
                    } else {
                        target.path = remove_dot_segments(&self.merge(&reference.path));
                    }
                    target.query.clone_from(&reference.query);
                }
                target.authority.clone_from(&self.authority);
            }
            target.scheme.clone_from(&self.scheme);
        }
        target
    }

    /// RFC 3986 section 5.2.3: the base path up to its last `/`, followed by the reference path.
    fn merge(&self, reference_path: &str) -> String {
        if self.authority.is_some() && self.path.is_empty() {
            return format!("/{reference_path}");
        }
        match self.path.rfind('/') {
            Some(last) => format!("{}{reference_path}", &self.path[..=last]),
            None => reference_path.to_string(),
        }
    }
}

impl fmt::Display for UriReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(scheme) = &self.scheme {
            write!(formatter, "{scheme}:")?;
        }
        if let Some(authority) = &self.authority {
            write!(formatter, "//{authority}")?;
        }
        formatter.write_str(&self.path)?;
        if let Some(query) = &self.query {
            write!(formatter, "?{query}")?;
        }
        if let Some(fragment) = &self.fragment {
            write!(formatter, "#{fragment}")?;
        }
        Ok(())
    }
}

fn split_at_first(text: &str, separator: char) -> (&str, Option<&str>) {
    match text.split_once(separator) {
        Some((before, after)) => (before, Some(after)),
        None => (text, None),
    }
}

/// The scheme before the first `:`, when what precedes it is a scheme, and the text after it.
fn split_scheme(text: &str) -> (Option<&str>, &str) {
    if let Some(colon) = text.find(':') {
        let candidate = &text[..colon];
        if is_scheme(candidate) {
            return (Some(candidate), &text[colon + 1..]);
        }
    }
    (None, text)
}

/// `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
fn is_scheme(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delim(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// `pchar` without the percent-escape: `unreserved / sub-delims / ":" / "@"`.
fn is_pchar(byte: u8) -> bool {
    is_unreserved(byte) || is_sub_delim(byte) || matches!(byte, b':' | b'@')
}

/// Whether `text` is made of percent-escapes and bytes that `allowed` accepts. Bytes outside ASCII
/// are never allowed: a URI has to escape them.
fn validate(text: &str, allowed: impl Fn(u8) -> bool) -> Result<(), UriError> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            let escape = bytes.get(index + 1..index + 3);
            if !escape.is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit)) {
                return Err(UriError::Escape);
            }
            index += 3;
        } else if byte.is_ascii() && allowed(byte) {
            index += 1;
        } else {
            return Err(UriError::Character);
        }
    }
    Ok(())
}

/// `authority = [ userinfo "@" ] host [ ":" port ]`
fn validate_authority(authority: &str) -> Result<(), UriError> {
    let host_and_port = match authority.split_once('@') {
        Some((userinfo, rest)) => {
            validate(userinfo, |byte| {
                is_unreserved(byte) || is_sub_delim(byte) || byte == b':'
            })?;
            rest
        }
        None => authority,
    };
    let (host, port) = if let Some(literal) = host_and_port.strip_prefix('[') {
        let Some((inside, after)) = literal.split_once(']') else {
            return Err(UriError::Authority);
        };
        validate_ip_literal(inside)?;
        let port = match after {
            "" => None,
            _ => Some(after.strip_prefix(':').ok_or(UriError::Authority)?),
        };
        (None, port)
    } else {
        match host_and_port.rsplit_once(':') {
            Some((host, port)) => (Some(host), Some(port)),
            None => (Some(host_and_port), None),
        }
    };
    if let Some(host) = host {
        validate(host, |byte| is_unreserved(byte) || is_sub_delim(byte))
            .map_err(|_| UriError::Authority)?;
    }
    if let Some(port) = port
        && !port.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(UriError::Authority);
    }
    Ok(())
}

/// The inside of `[...]`: an IPv6 address or `IPvFuture = "v" 1*HEXDIG "." 1*( unreserved / sub-delims / ":" )`.
fn validate_ip_literal(inside: &str) -> Result<(), UriError> {
    if let Some(future) = inside.strip_prefix(['v', 'V']) {
        let Some((version, rest)) = future.split_once('.') else {
            return Err(UriError::Authority);
        };
        let valid = !version.is_empty()
            && version.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !rest.is_empty()
            && rest
                .bytes()
                .all(|byte| is_unreserved(byte) || is_sub_delim(byte) || byte == b':');
        return if valid { Ok(()) } else { Err(UriError::Authority) };
    }
    Ipv6Addr::from_str(inside)
        .map(|_| ())
        .map_err(|_| UriError::Authority)
}

/// RFC 3986 section 5.2.4, over slices of the input so that the cost is linear in its length.
fn remove_dot_segments(path: &str) -> String {
    let mut rest = path;
    let mut output = String::with_capacity(path.len());
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("../") {
            rest = after;
        } else if let Some(after) = rest.strip_prefix("./") {
            rest = after;
        } else if rest.starts_with("/./") {
            rest = &rest[2..];
        } else if rest == "/." {
            rest = "/";
        } else if rest.starts_with("/../") {
            rest = &rest[3..];
            pop_segment(&mut output);
        } else if rest == "/.." {
            rest = "/";
            pop_segment(&mut output);
        } else if rest == "." || rest == ".." {
            rest = "";
        } else {
            let start = usize::from(rest.starts_with('/'));
            let end = rest[start..]
                .find('/')
                .map_or(rest.len(), |slash| start + slash);
            output.push_str(&rest[..end]);
            rest = &rest[end..];
        }
    }
    output
}

/// Removes the last segment of `output` together with the `/` in front of it.
fn pop_segment(output: &mut String) {
    match output.rfind('/') {
        Some(slash) => output.truncate(slash),
        None => output.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(base: &str, reference: &str) -> String {
        let base = UriReference::parse_absolute(base).unwrap();
        let reference = UriReference::parse(reference).unwrap();
        base.resolve(&reference).to_string()
    }

    #[test]
    fn resolves_the_normal_examples_of_rfc_3986_section_5_4_1() {
        let base = "http://a/b/c/d;p?q";
        for (reference, expected) in [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            ("g?y#s", "http://a/b/c/g?y#s"),
            (";x", "http://a/b/c/;x"),
            ("g;x", "http://a/b/c/g;x"),
            ("g;x?y#s", "http://a/b/c/g;x?y#s"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
        ] {
            assert_eq!(resolved(base, reference), expected, "reference {reference:?}");
        }
    }

    #[test]
    fn resolves_the_abnormal_examples_of_rfc_3986_section_5_4_2() {
        let base = "http://a/b/c/d;p?q";
        for (reference, expected) in [
            ("../../../g", "http://a/g"),
            ("../../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("g..", "http://a/b/c/g.."),
            ("..g", "http://a/b/c/..g"),
            ("./../g", "http://a/b/g"),
            ("./g/.", "http://a/b/c/g/"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
            ("g?y/./x", "http://a/b/c/g?y/./x"),
            ("g?y/../x", "http://a/b/c/g?y/../x"),
            ("g#s/./x", "http://a/b/c/g#s/./x"),
            ("g#s/../x", "http://a/b/c/g#s/../x"),
            ("http:g", "http:g"),
        ] {
            assert_eq!(resolved(base, reference), expected, "reference {reference:?}");
        }
    }

    #[test]
    fn resolves_against_a_base_without_a_path_and_with_an_authority() {
        assert_eq!(resolved("http://a", "g"), "http://a/g");
        assert_eq!(resolved("http://a", "?q"), "http://a?q");
        assert_eq!(resolved("file:///x/y.json", "../z/"), "file:///z/");
        assert_eq!(
            resolved("file:///__dartscope_project__/a/.dart_tool/package_config.json", "../"),
            "file:///__dartscope_project__/a/"
        );
    }

    #[test]
    fn keeps_escapes_and_case_as_written() {
        assert_eq!(
            resolved("file:///cache/my%20package/", "lib%20src/Api%2fX.dart"),
            "file:///cache/my%20package/lib%20src/Api%2fX.dart"
        );
        // An escaped dot is not a dot segment; callers that care decode it.
        assert_eq!(
            resolved("file:///a/b/", "%2e%2e/c"),
            "file:///a/b/%2e%2e/c"
        );
        let reference = UriReference::parse("FILE://Host/A").unwrap();
        assert_eq!(reference.scheme(), Some("FILE"));
        assert_eq!(reference.authority(), Some("Host"));
        assert_eq!(reference.to_string(), "FILE://Host/A");
    }

    #[test]
    fn splits_the_components() {
        let reference = UriReference::parse("https://user:pw@example.com:8080/a/b?x=1&y=2#frag").unwrap();
        assert_eq!(reference.scheme(), Some("https"));
        assert_eq!(reference.authority(), Some("user:pw@example.com:8080"));
        assert_eq!(reference.path, "/a/b");
        assert_eq!(reference.query(), Some("x=1&y=2"));
        assert_eq!(reference.fragment(), Some("frag"));

        let relative = UriReference::parse("../lib/a.dart").unwrap();
        assert_eq!(relative.scheme(), None);
        assert_eq!(relative.authority(), None);
        assert_eq!(relative.query(), None);
        assert_eq!(relative.fragment(), None);

        let empty = UriReference::parse("").unwrap();
        assert_eq!(empty.to_string(), "");
        assert_eq!(empty.query(), None);

        let present_but_empty = UriReference::parse("a?#").unwrap();
        assert_eq!(present_but_empty.query(), Some(""));
        assert_eq!(present_but_empty.fragment(), Some(""));
    }

    #[test]
    fn accepts_what_rfc_3986_allows() {
        for valid in [
            "file:///C:/Users/demo/app/",
            "file:///C%3A/Users/demo/app/",
            "package:app/src/a.dart",
            "a:b",
            "mailto:someone@example.com",
            "urn:isbn:0451450523",
            "//host/path",
            "//host:/path",
            "//[::1]/path",
            "//[v7.a:b]/path",
            "//[2001:db8::7]:8080/",
            "//u@h",
            "/a/b/../c",
            "a/b:c",
            "./a:b",
            "?q",
            "#f",
            "a//b",
            "a?b?c",
            "a#b/c?d",
            "%41%7a",
            "a;b=c,d",
        ] {
            assert!(UriReference::parse(valid).is_ok(), "{valid:?} should be accepted");
        }
    }

    #[test]
    fn rejects_what_rfc_3986_forbids() {
        for invalid in [
            "a b",
            "a\tb",
            "a\\b",
            "a^b",
            "a{b}",
            "a|b",
            "a`b",
            "a\"b",
            "a<b>",
            "caf\u{e9}",
            "%",
            "%4",
            "%zz",
            "a%2",
            "a#b#c",
            ":a",
            "1a:b/c:d",
            "a b:c",
            "x/y:z/../:w ",
            "//host:80a/",
            "//[::1/",
            "//[::1]x/",
            "//[::g]/",
            "//[v.a]/",
            "//[vz.a]/",
            "//a@b@c/",
            "//ho st/",
            "//ho[st/",
            "//user pass@h/",
            "//h/a b",
            "a?b c",
            "a#b c",
        ] {
            assert!(UriReference::parse(invalid).is_err(), "{invalid:?} should be rejected");
        }
        assert_eq!(
            UriReference::parse_absolute("../a"),
            Err(UriError::MissingScheme)
        );
    }

    #[test]
    fn dot_segments_are_removed_in_linear_time() {
        let long = format!("/{}", "a/../".repeat(200_000));
        assert_eq!(remove_dot_segments(&long), "/");
        let deep = format!("{}x", "./".repeat(200_000));
        assert_eq!(remove_dot_segments(&deep), "x");
        let slashes = "/.".repeat(200_000);
        assert_eq!(remove_dot_segments(&slashes), "/");
    }
}
