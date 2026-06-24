use regex::Regex;
use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn sanitize_log(input: &str) -> String {
    let mut output = input.to_string();
    let patterns = [
        (
            r"(?i)(token|secret|password|api[_-]?key)\s*[:=]\s*[^\s,;]+",
            "$1=[REDACTED]",
        ),
        (r"(?i)bearer\s+[a-z0-9._\-]+", "Bearer [REDACTED]"),
        (
            r"(?i)[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}",
            "[REDACTED_EMAIL]",
        ),
        (r"\b\d{1,2}\.?\d{3}\.?\d{3}-[\dkK]\b", "[REDACTED_ID]"),
    ];

    for (pattern, replacement) in patterns {
        let regex = Regex::new(pattern).expect("valid sanitizer pattern");
        output = regex.replace_all(&output, replacement).to_string();
    }

    if output.len() > 12_000 {
        output.truncate(12_000);
        output.push_str("\n[TRUNCATED]");
    }

    output
}

#[allow(dead_code)]
pub fn blocked_command_reason(command: &str) -> Option<&'static str> {
    let normalized = command.to_ascii_lowercase();
    let blocked = [
        "git reset --hard",
        "git clean",
        "push --force",
        "git push",
        "remove-item -recurse",
        "rm -rf",
        "del /s",
    ];
    blocked
        .iter()
        .find(|needle| normalized.contains(**needle))
        .map(|_| "Comando bloqueado por safety kernel.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_redacts_common_sensitive_values() {
        let text = "token=abc test@example.com 12.345.678-9";
        let sanitized = sanitize_log(text);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(sanitized.contains("[REDACTED_EMAIL]"));
        assert!(sanitized.contains("[REDACTED_ID]"));
    }

    #[test]
    fn blocked_commands_are_detected() {
        assert!(blocked_command_reason("git reset --hard").is_some());
        assert!(blocked_command_reason("npm run check").is_none());
    }
}
