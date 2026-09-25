//! Canonical Dart identifier rules.
//!
//! A Dart identifier is an `IDENTIFIER_START` byte followed by any number of `IDENTIFIER_PART`
//! bytes, where `IDENTIFIER_START` is an ASCII letter, `_`, or `$` and `IDENTIFIER_PART` additionally
//! allows ASCII digits. The dollar sign matters in real sources: generated bindings, code generation
//! helpers, and framework internals rely on names such as `_$UserFromJson`, `$Experimental`, and
//! `jni$_`. Every scanner that recognizes a Dart name uses this module so no name is truncated at a
//! dollar sign.
//!
//! GraphQL names are a different language and do not accept `$`
//! (`Name ::= /[_A-Za-z][_0-9A-Za-z]*/`), so GraphQL document parsing keeps its own predicates.

/// Returns whether `byte` may start a Dart identifier.
pub(crate) fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

/// Returns whether `byte` may continue a Dart identifier.
pub(crate) fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

/// Returns the exclusive end of the identifier that starts at `from`, scanning to the end of `bytes`.
pub(crate) fn identifier_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while bytes
        .get(at)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        at += 1;
    }
    at
}

/// Returns the identifier that `value` starts with, ignoring anything that follows it.
pub(crate) fn leading_identifier(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    if !bytes.first().is_some_and(|byte| is_identifier_start(*byte)) {
        return None;
    }
    Some(&value[..identifier_end(bytes, 0)])
}

/// Returns whether `value` is exactly one Dart identifier.
pub(crate) fn is_identifier(value: &str) -> bool {
    leading_identifier(value) == Some(value)
}

#[cfg(test)]
mod tests {
    use super::{identifier_end, is_identifier, leading_identifier};

    #[test]
    fn accepts_dollar_signs_and_rejects_other_characters() {
        assert!(is_identifier("name"));
        assert!(is_identifier("_"));
        assert!(is_identifier("$"));
        assert!(is_identifier("_$UserFromJson"));
        assert!(is_identifier("jni$_"));
        assert!(is_identifier("count$"));
        assert!(is_identifier("A1$b"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("1name"));
        assert!(!is_identifier("-name"));
        assert!(!is_identifier("na me"));
        assert!(!is_identifier("na.me"));
        assert!(!is_identifier("na-"));
    }

    #[test]
    fn scans_a_leading_identifier_without_truncating_it() {
        assert_eq!(
            leading_identifier("_$jniVersionCheck = 1"),
            Some("_$jniVersionCheck")
        );
        assert_eq!(leading_identifier("value$;"), Some("value$"));
        assert_eq!(leading_identifier("1name"), None);
        assert_eq!(
            identifier_end(b"_$UserFromJson(", 0),
            "_$UserFromJson".len()
        );
    }
}
