use nexus::testing::{
    parse_lsof_cwds_multi, parse_project_session, parse_ps_output, parse_transcript,
    parse_worktree_list,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn ps_output_parser_never_panics(input in ".*") {
        let _ = parse_ps_output(&input);
    }

    #[test]
    fn lsof_cwds_multi_parser_never_panics(input in ".*") {
        let _ = parse_lsof_cwds_multi(&input);
    }

    #[test]
    fn worktree_parser_never_panics(input in ".*") {
        let _ = parse_worktree_list(&input);
    }

    #[test]
    fn transcript_parser_never_panics(input in ".*") {
        let _ = parse_transcript(&input);
    }

    #[test]
    fn project_session_parser_never_panics(input in ".*") {
        let _ = parse_project_session(&input);
    }
}
