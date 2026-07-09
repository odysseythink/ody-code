use agent_rs::session_mode::topic_generator::*;

#[test]
fn slugify_handles_empty() {
    assert_eq!(slugify_title(""), "");
}

#[test]
fn slugify_handles_only_special() {
    assert_eq!(slugify_title("!@#$%"), "");
}

#[test]
fn strip_locators_removes_paths_and_urls() {
    let input = "Read /home/user/file.txt and https://example.com/page for info";
    let result = strip_locators(input);
    assert!(!result.contains("/home/user"));
    assert!(!result.contains("https://"));
    assert!(result.contains("for info"));
}
