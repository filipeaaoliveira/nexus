use proptest::prelude::*;

proptest! {
    #[test]
    fn ps_output_parser_never_panics(input in ".*") {
        let _ = nexus::detect::process::parse_ps_output(&input);
    }

    #[test]
    fn lsof_cwds_multi_parser_never_panics(input in ".*") {
        let _ = nexus::detect::process::parse_lsof_cwds_multi(&input);
    }

    #[test]
    fn worktree_parser_never_panics(input in ".*") {
        let _ = nexus::detect::worktree::parse_worktree_list(&input);
    }

    #[test]
    fn transcript_parser_never_panics(input in ".*") {
        let _ = nexus::detect::claude::parse_transcript(&input);
    }

    #[test]
    fn project_session_parser_never_panics(input in ".*") {
        let _ = nexus::detect::claude::parse_project_session(&input);
    }
}
