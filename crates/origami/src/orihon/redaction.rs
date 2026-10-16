use regex::Regex;
use std::sync::LazyLock;

const CREDENTIAL_KEY: &str = r"password|passwd|secret|token|api[_-]?key|authorization|cookie|credential|private[_-]?key|database[_-]?(?:url|dsn)|connection[_-]?string";
static SECRET_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?i)(?:{CREDENTIAL_KEY})$")).expect("fixed credential key pattern")
});

static AUTHENTICATED_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([a-z][a-z0-9+.-]*://)[^/\s@]+@").expect("fixed authenticated URL pattern")
});
static AUTH_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\b(?:authorization|proxy-authorization)\s*:\s*(?:Bearer|Basic|Token)\s+)[A-Za-z0-9._~+/=-]+")
        .expect("fixed authorization header pattern")
});
static AUTH_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(Bearer|Basic|Token)\s+[A-Za-z0-9._~+/=-]{16,}")
        .expect("fixed standalone authorization token pattern")
});
static QUOTED_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?i)\b([A-Za-z0-9_.-]*(?:{CREDENTIAL_KEY})\s*=\s*)(?:"(?:\\.|[^"\\])*"|'[^']*')"#
    ))
    .expect("fixed credential assignment pattern")
});
static ENV_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?i)\b([A-Za-z0-9_.-]*(?:{CREDENTIAL_KEY})=)[^\s,;"'\\}}=>(][^\s,;"'\\}})]*"#
    ))
    .expect("fixed environment credential pattern")
});
static JSON_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?i)((?:["']?)\b[A-Za-z0-9_.-]*(?:{CREDENTIAL_KEY})(?:["']?)\s*:\s*)(?:"(?:\\.|[^"\\])*"|'[^']*')"#
    ))
    .expect("fixed JSON credential pattern")
});

pub(super) fn redact(value: String) -> (String, bool) {
    if value.trim_start().starts_with(['{', '['])
        && serde_json::from_str::<serde::de::IgnoredAny>(&value).is_ok()
    {
        return match redact_json_strings(&value) {
            Some(redacted) => (redacted, true),
            None => (value, false),
        };
    }
    redact_text(value)
}

fn redact_text(value: String) -> (String, bool) {
    // Host removes its exact private values first. These credential patterns
    // also protect direct callers of either store; they do not clip evidence.
    let url = AUTHENTICATED_URL.replace_all(&value, "${1}[REDACTED]@");
    let header = AUTH_HEADER.replace_all(&url, "${1}[REDACTED]");
    let token = AUTH_TOKEN.replace_all(&header, "${1} [REDACTED]");
    let quoted = QUOTED_ASSIGNMENT.replace_all(&token, "${1}[REDACTED]");
    let assignment = ENV_ASSIGNMENT.replace_all(&quoted, "${1}[REDACTED]");
    let json = JSON_FIELD.replace_all(&assignment, "${1}\"[REDACTED]\"");
    if json == value {
        (value, false)
    } else {
        (json.into_owned(), true)
    }
}

fn unicode_escape(value: &str, start: usize) -> Option<u16> {
    let escape = value.get(start..start.checked_add(6)?)?;
    escape
        .strip_prefix(r"\u")
        .and_then(|hex| u16::from_str_radix(hex, 16).ok())
}

fn decode_json_string_lossy(value: &str) -> Result<String, serde_json::Error> {
    let error = match serde_json::from_str::<String>(value) {
        Ok(decoded) => return Ok(decoded),
        Err(error) => error,
    };
    let mut repaired: Option<String> = None;
    let mut copied = 0;
    let mut position = 0;
    let bytes = value.as_bytes();
    while position + 1 < bytes.len() {
        if bytes[position] != b'\\' {
            position += 1;
            continue;
        }
        if bytes[position + 1] != b'u' {
            position += 2;
            continue;
        }
        let Some(code) = unicode_escape(value, position) else {
            return Err(error);
        };
        if (0xD800..=0xDBFF).contains(&code)
            && unicode_escape(value, position + 6)
                .is_some_and(|next| (0xDC00..=0xDFFF).contains(&next))
        {
            position += 12;
            continue;
        }
        if (0xD800..=0xDFFF).contains(&code) {
            let target = repaired.get_or_insert_with(|| String::with_capacity(value.len()));
            target.push_str(&value[copied..position]);
            target.push_str(r"\ufffd");
            copied = position + 6;
        }
        position += 6;
    }
    match repaired {
        Some(mut repaired) => {
            repaired.push_str(&value[copied..]);
            serde_json::from_str(&repaired)
        }
        None => Err(error),
    }
}

fn redact_json_strings(value: &str) -> Option<String> {
    // A Value map would discard duplicate keys. Keep every byte outside changed strings.
    let mut output: Option<String> = None;
    let mut copied = 0;
    let mut position = 0;
    let mut secret = false;
    let bytes = value.as_bytes();
    while let Some(offset) = value[position..].find('"') {
        let start = position + offset;
        let mut end = start + 1;
        while bytes[end] != b'"' {
            if bytes[end] == b'\\' {
                end += 1;
            }
            end += 1;
        }
        end += 1;
        let after_colon = value[end..].trim_start().strip_prefix(':');
        let (redacted, changed) = if secret && after_colon.is_none() {
            secret = false;
            (
                "[REDACTED]".to_owned(),
                &value[start..end] != r#""[REDACTED]""#,
            )
        } else {
            let Ok(decoded) = decode_json_string_lossy(&value[start..end]) else {
                // Invalid escape syntax stays opaque; other strings still receive redaction.
                secret = false;
                position = end;
                continue;
            };
            if let Some(after) = after_colon {
                secret = SECRET_KEY.is_match(&decoded) && after.trim_start().starts_with('"');
                redact_text(decoded)
            } else {
                secret = false;
                redact(decoded)
            }
        };
        if changed {
            let target = output.get_or_insert_with(|| String::with_capacity(value.len()));
            target.push_str(&value[copied..start]);
            target.push_str(&serde_json::to_string(&redacted).expect("string serializes"));
            copied = end;
        }
        position = end;
    }
    if let Some(target) = &mut output {
        target.push_str(&value[copied..]);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn credential_redaction_preserves_json_evidence_and_plain_text() {
        let cases = [
            (
                r#"{"text":"API_KEY=sk-1\nHOST=db\nPORT=5432"}"#,
                r#"{"text":"API_KEY=[REDACTED]\nHOST=db\nPORT=5432"}"#,
            ),
            (
                r#"{"text":"password=\"ab\\\"cd\"\nHOST=db"}"#,
                r#"{"text":"password=[REDACTED]\nHOST=db"}"#,
            ),
            (
                r#"{ "keep":1, "token":"abc", "keep":2 }"#,
                r#"{ "keep":1, "token":"[REDACTED]", "keep":2 }"#,
            ),
            (
                r#"{"headers":{"X-Api-Key":"sk-live-123","Set-Cookie":"abc"}}"#,
                r#"{"headers":{"X-Api-Key":"[REDACTED]","Set-Cookie":"[REDACTED]"}}"#,
            ),
            (
                r#"{"access_token":"alpha","dbPassword":"beta"}"#,
                r#"{"access_token":"[REDACTED]","dbPassword":"[REDACTED]"}"#,
            ),
            (
                r#"{"postgres://admin:hunter2@db/app":"reachable"}"#,
                r#"{"postgres://[REDACTED]@db/app":"reachable"}"#,
            ),
            (
                r#"{"text":"\ud83d\ude00 \ud800 DB_PASSWORD=hunter2"}"#,
                "{\"text\":\"\u{1F600} \u{FFFD} DB_PASSWORD=[REDACTED]\"}",
            ),
            (
                r#"{"bad":"\ud800","text":"line\npassword=hunter2\nHOST=db"}"#,
                r#"{"bad":"\ud800","text":"line\npassword=[REDACTED]\nHOST=db"}"#,
            ),
            (
                "if (accessToken === undefined) return; const refreshToken = await getRefreshToken(); const getToken = () => x;",
                "if (accessToken === undefined) return; const refreshToken = await getRefreshToken(); const getToken = () => x;",
            ),
            (r#"{"text":"\ud800"}"#, r#"{"text":"\ud800"}"#),
            (r#"[{"token":1},"plain"]"#, r#"[{"token":1},"plain"]"#),
            (
                "the basic idea; the token limit; bearer of news; token: string; pub password: String,",
                "the basic idea; the token limit; bearer of news; token: string; pub password: String,",
            ),
            (
                "psycopg2.connect(host=h, password=pw)",
                "psycopg2.connect(host=h, password=[REDACTED])",
            ),
        ];
        for (input, expected) in cases {
            let (actual, changed) = redact(input.into());
            assert_eq!(actual, expected, "input: {input}");
            assert_eq!(changed, input != expected, "input: {input}");
        }
    }
}
