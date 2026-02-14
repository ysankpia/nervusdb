pub(super) fn strip_explain_prefix(input: &str) -> Option<&str> {
    let trimmed = input.trim_start();
    let prefix_len = "EXPLAIN".len();
    if trimmed.len() < prefix_len {
        return None;
    }
    let head = trimmed.get(..prefix_len)?;
    if !head.eq_ignore_ascii_case("EXPLAIN") {
        return None;
    }
    let tail = trimmed.get(prefix_len..)?;
    if let Some(next) = tail.chars().next()
        && !next.is_whitespace()
    {
        // Avoid matching `EXPLAINED`, etc.
        return None;
    }
    Some(tail.trim_start())
}

#[cfg(test)]
mod tests {
    use super::strip_explain_prefix;

    #[test]
    fn accepts_explain_case_insensitive() {
        assert_eq!(
            strip_explain_prefix("EXPLAIN MATCH (n) RETURN n"),
            Some("MATCH (n) RETURN n")
        );
        assert_eq!(strip_explain_prefix("explain  RETURN 1"), Some("RETURN 1"));
    }

    #[test]
    fn rejects_non_explain_prefix() {
        assert_eq!(strip_explain_prefix("EXPLAINED RETURN 1"), None);
        assert_eq!(strip_explain_prefix("PROFILE RETURN 1"), None);
    }

    #[test]
    fn handles_whitespace_only_payload() {
        assert_eq!(strip_explain_prefix("EXPLAIN"), Some(""));
        assert_eq!(strip_explain_prefix("   EXPLAIN   "), Some(""));
    }
}
