//! Sanitized printing helpers.
//!
//! Every value that reaches a terminal from this crate passes through one of
//! these helpers. The allowlist is the point: the doctor and status output is
//! the paste surface for a support request, so it carries versions, counters,
//! and states, and nothing else. No design content, no file identifier, and no
//! credential is printed here, with one deliberate exception: [`secret`], used
//! by the explicit local pair action only.

use std::io::{self, Write};

/// Print a labelled value.
pub fn field(name: &str, value: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "  {name:<24} {value}");
}

/// Print a section heading.
pub fn section(title: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out);
    let _ = writeln!(out, "{title}");
}

/// Print a plain line.
pub fn line(text: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{text}");
}

/// Print an empty line.
pub fn blank() {
    let mut out = io::stdout().lock();
    let _ = writeln!(out);
}

/// Print a note.
pub fn note(text: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "note: {text}");
}

/// Print an error on the error stream.
pub fn error(text: &str) {
    let mut err = io::stderr().lock();
    let _ = writeln!(err, "error: {text}");
}

/// Print a JSON document on the standard output.
pub fn json<T: serde::Serialize>(value: &T) -> Result<(), serde_json::Error> {
    let text = serde_json::to_string_pretty(value)?;
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{text}");
    Ok(())
}

/// Print a one time secret to the terminal.
///
/// The secret goes to the standard output and nowhere else. It is never
/// written to a log file and never returned by any other command.
pub fn secret(value: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out);
    let _ = writeln!(out, "    {value}");
    let _ = writeln!(out);
}

/// Mask credential shaped tokens inside free text.
///
/// A token is masked when it is at least 32 characters long and consists of
/// letters, digits, underscore, and dash only. That covers the hexadecimal
/// encoding of the 256 bit pairing secret and the base64 encoding of a
/// credential, and it leaves ordinary words and paths alone. The original
/// spacing and line breaks are kept, so a multi line message stays readable.
pub fn redact(text: &str) -> String {
    let mut masked = String::with_capacity(text.len());
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            push_token(&mut masked, &text[start..index]);
            masked.push(ch);
            start = index + ch.len_utf8();
        }
    }
    push_token(&mut masked, &text[start..]);
    masked
}

/// Append one token, masked when it looks like a credential.
fn push_token(masked: &mut String, token: &str) {
    if is_secret_shaped(token) {
        masked.push_str("***redacted***");
    } else {
        masked.push_str(token);
    }
}

/// Whether a token looks like a credential.
fn is_secret_shaped(token: &str) -> bool {
    token.len() >= 32
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_token_is_masked() {
        let secret = "a".repeat(64);
        let text = redact(&format!("the secret is {secret} here"));
        assert!(!text.contains(&secret));
        assert!(text.contains("***redacted***"));
    }

    #[test]
    fn ordinary_words_survive() {
        let text = redact("the runtime directory is /tmp/figtether and ready");
        assert_eq!(text, "the runtime directory is /tmp/figtether and ready");
    }

    #[test]
    fn the_line_breaks_survive() {
        let text = redact("first line\nsecond line");
        assert_eq!(text, "first line\nsecond line");
    }

    #[test]
    fn a_short_token_survives() {
        assert_eq!(redact("port 38947 is free"), "port 38947 is free");
    }
}
