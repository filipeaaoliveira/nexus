use ai_overview::detect::opencode::*;

#[test]
fn test_parse_message_data_user() {
    let json = r#"{"role":"user","time":{"created":1774988071935},"agent":"general","model":{"providerID":"github-copilot","modelID":"claude-opus-4.6"}}"#;
    let data = parse_message_data(json);
    assert!(data.is_some());
    let d = data.unwrap();
    assert_eq!(d.role, "user");
    assert_eq!(d.agent, Some("general".to_string()));
    assert_eq!(d.model_id, Some("claude-opus-4.6".to_string()));
}

#[test]
fn test_parse_message_data_assistant() {
    let json = r#"{"role":"assistant","parentID":"msg_abc","mode":"general","agent":"explore","path":{"cwd":"/tmp","root":"/tmp"},"modelID":"claude-opus-4.6","providerID":"github-copilot","time":{"created":1774988071939}}"#;
    let data = parse_message_data(json);
    assert!(data.is_some());
    let d = data.unwrap();
    assert_eq!(d.role, "assistant");
    assert_eq!(d.agent, Some("explore".to_string()));
}

#[test]
fn test_parse_part_data_tool() {
    let json = r#"{"type":"tool","tool":"bash","callID":"tooluse_abc","state":{"status":"completed","input":{"command":"npm test"},"output":"All tests passed"}}"#;
    let part = parse_part_data(json);
    assert!(part.is_some());
    let p = part.unwrap();
    assert_eq!(p.part_type, "tool");
    assert_eq!(p.tool_name, Some("bash".to_string()));
    assert_eq!(p.tool_status, Some("completed".to_string()));
    assert_eq!(p.tool_input_summary, Some("npm test".to_string()));
}

#[test]
fn test_parse_part_data_text() {
    let json =
        r#"{"type":"text","text":"I'll help you with that.","time":{"start":1774988083425}}"#;
    let part = parse_part_data(json);
    assert!(part.is_some());
    let p = part.unwrap();
    assert_eq!(p.part_type, "text");
}

#[test]
fn test_parse_part_data_step_finish() {
    let json = r#"{"type":"step-finish","snapshot":"abc123","reason":"tool-calls","tokens":{"total":34898},"cost":0}"#;
    let part = parse_part_data(json);
    assert!(part.is_some());
    let p = part.unwrap();
    assert_eq!(p.part_type, "step-finish");
}

#[test]
fn test_session_is_subagent() {
    assert!(is_subagent_title("Fix the tests (@explore subagent)"));
    assert!(is_subagent_title("Search codebase (@general subagent)"));
    assert!(!is_subagent_title("Fix the tests"));
    assert!(!is_subagent_title("Regular session title"));
}

#[test]
fn test_extract_subagent_type() {
    assert_eq!(
        extract_subagent_type("Fix tests (@explore subagent)"),
        Some("explore")
    );
    assert_eq!(
        extract_subagent_type("Search (@general subagent)"),
        Some("general")
    );
    assert_eq!(extract_subagent_type("Normal title"), None);
}
