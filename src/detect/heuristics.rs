/// Returns true if `line` (after stripping leading markdown/whitespace characters) matches
/// `marker` as an exact word, a header, or the start of a phrase.
pub(crate) fn contains_review_section_marker(text: &str, marker: &str) -> bool {
    text.lines().map(str::trim_start).any(|line| {
        let line = line.trim_start_matches(['#', '>', '-', '*', ' ', '\t']);
        let lower = line.to_ascii_lowercase();

        lower == marker
            || lower.starts_with(&format!("{marker}:"))
            || lower.starts_with(&format!("{marker} -"))
            || lower.starts_with(&format!("{marker} "))
    })
}

/// Heuristically decides whether `text` looks like a code-review output rather
/// than a normal agent response. Used to set the `NormalizedReason::Reviewing`
/// state across all detectors.
pub(crate) fn looks_like_review_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();

    // Structured section headings common across code-review formats (GitHub,
    // Gerrit, AI-generated reviews, etc.).  Markers are intentionally broad so
    // that detection degrades gracefully if any single format changes.
    if [
        "### issues",
        "#### critical",
        "ready to merge",
        "merge-readiness",
        "review assessment",
        "review findings",
        "audit findings",
        "security findings",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }

    [
        "requested changes",
        "changes requested",
        "approve with comments",
    ]
    .iter()
    .any(|marker| contains_review_section_marker(trimmed, marker))
}

#[cfg(test)]
mod tests {
    use super::looks_like_review_output;

    #[test]
    fn detects_issues_section() {
        assert!(looks_like_review_output("### Issues\nsome content"));
    }

    #[test]
    fn detects_critical_section() {
        assert!(looks_like_review_output("#### Critical\nsome bug"));
    }

    #[test]
    fn detects_ready_to_merge() {
        assert!(looks_like_review_output("Ready to merge? Yes"));
    }

    #[test]
    fn detects_merge_readiness() {
        assert!(looks_like_review_output("Merge-readiness assessment: good"));
    }

    #[test]
    fn detects_security_findings() {
        assert!(looks_like_review_output("Security findings: none"));
    }

    #[test]
    fn detects_changes_requested() {
        assert!(looks_like_review_output("## Changes requested\nplease fix"));
    }

    #[test]
    fn detects_approve_with_comments() {
        assert!(looks_like_review_output(
            "approve with comments: minor nits"
        ));
    }

    #[test]
    fn normal_response_is_not_review() {
        assert!(!looks_like_review_output("I've implemented the feature."));
        assert!(!looks_like_review_output(""));
    }
}
