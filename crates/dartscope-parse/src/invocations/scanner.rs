#[derive(Debug, Clone)]
pub(super) struct CallCandidate {
    pub(super) target: String,
    pub(super) start: usize,
    pub(super) open: usize,
    pub(super) close: usize,
    pub(super) end: usize,
    pub(super) result_members: Vec<String>,
}

pub(super) fn scan_call_candidates(source: &str) -> Vec<CallCandidate> {
    let bytes = source.as_bytes();
    let mut candidates = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if is_identifier_start(bytes[index]) && is_chain_start(bytes, index) {
            candidates.extend(scan_chain(source, index));
            index = identifier_end(bytes, index);
        } else {
            index += 1;
        }
    }
    candidates
}

fn scan_chain(source: &str, start: usize) -> Vec<CallCandidate> {
    let bytes = source.as_bytes();
    let first_end = identifier_end(bytes, start);
    let first = &source[start..first_end];
    if is_reserved_target(first) {
        return Vec::new();
    }

    let mut parts = vec![first.to_string()];
    let mut cursor = first_end;
    let mut calls: Vec<CallCandidate> = Vec::new();

    loop {
        cursor = skip_whitespace(bytes, cursor);
        cursor = skip_type_arguments(source, cursor).unwrap_or(cursor);
        cursor = skip_whitespace(bytes, cursor);

        if bytes.get(cursor) == Some(&b'(') {
            let Some(close) = matching_delimiter(source, cursor, b'(', b')') else {
                break;
            };
            calls.push(CallCandidate {
                target: parts.join("."),
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
        let after_type_arguments = skip_type_arguments(source, cursor).unwrap_or(cursor);
        let after = skip_whitespace(bytes, after_type_arguments);
        if bytes.get(after) != Some(&b'(')
            && let Some(call) = calls.last_mut()
        {
            call.result_members.push(member);
            call.end = member_end;
        }
        cursor = after_type_arguments;
    }

    calls
}

pub(super) fn matching_delimiter(
    source: &str,
    open: usize,
    open_byte: u8,
    close_byte: u8,
) -> Option<usize> {
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

fn skip_type_arguments(source: &str, at: usize) -> Option<usize> {
    if source.as_bytes().get(at) != Some(&b'<') {
        return None;
    }
    let close = matching_delimiter(source, at, b'<', b'>')?;
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
    !matches!(bytes[at - 1], b'.' | b'$') && !is_identifier_continue(bytes[at - 1])
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

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
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
    use super::scan_call_candidates;

    #[test]
    fn scans_chained_and_result_member_calls() {
        let source = "DefaultAssetBundle.of(context).loadString(          ); AppLocalizations.of(context)!.welcomeMessage";
        let calls = scan_call_candidates(source);
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
}
