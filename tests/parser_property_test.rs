use nexus::testing::{
    parse_legacy_status_output, parse_lsof_cwds_multi, parse_message_data, parse_part_data,
    parse_project_session, parse_ps_output, parse_rollout, parse_session_index, parse_source_field,
    parse_transcript, parse_worktree_list,
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

    #[test]
    fn rollout_parser_never_panics(input in ".*") {
        let _ = parse_rollout(&input);
    }

    #[test]
    fn session_index_parser_never_panics(input in ".*") {
        let _ = parse_session_index(&input);
    }

    #[test]
    fn source_field_parser_never_panics(input in ".*") {
        let _ = parse_source_field(&input);
    }

    #[test]
    fn opencode_message_parser_never_panics(input in ".*") {
        let _ = parse_message_data(&input);
    }

    #[test]
    fn opencode_part_parser_never_panics(input in ".*") {
        let _ = parse_part_data(&input, 0);
    }

    #[test]
    fn legacy_status_parser_never_panics(input in ".*") {
        let _ = parse_legacy_status_output(&input);
    }
}
