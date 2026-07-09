use agent_rs::injection::parts_manifest::*;

#[test]
fn test_parse_parts_manifest_with_pending() {
    let content = "| # | File | Scope | Status |\n|---|---|---|---|\n| 1 | core.md | core | done |\n| 2 | api.md | api | pending |\n| 3 | test.md | test | pending |";
    let result = parse_parts_manifest(content);
    assert!(result.is_some());
    let manifest = result.unwrap();
    assert!(!manifest.all_done);
    assert!(manifest.next.is_some());
    assert_eq!(manifest.next.unwrap().file, "api.md");
}

#[test]
fn test_parse_parts_manifest_all_done() {
    let content = "| # | File | Scope | Status |\n|---|---|---|---|\n| 1 | core.md | core | done |\n| 2 | api.md | api | done |";
    let result = parse_parts_manifest(content);
    assert!(result.is_some());
    let manifest = result.unwrap();
    assert!(manifest.all_done);
    assert!(manifest.next.is_none());
}

#[test]
fn test_parse_manifest_files() {
    let content = "| # | File | Scope | Status |\n|---|---|---|---|\n| 1 | core.md | core | done |\n| 2 | api.md | api | pending |";
    let files = parse_manifest_files(content);
    assert_eq!(files, vec!["core.md", "api.md"]);
}

#[test]
fn test_count_manifest_rows() {
    let content = "| # | File | Scope | Status |\n|---|---|---|---|\n| 1 | core.md | core | done |\n| 2 | api.md | api | pending |\n\nSome trailing text";
    let counts = count_manifest_rows(content);
    assert!(counts.is_some());
    let (done_count, pending_count) = counts.unwrap();
    assert_eq!(done_count, 1);
    assert_eq!(pending_count, 1);
}

#[test]
fn test_parse_parts_manifest_empty() {
    assert!(parse_parts_manifest("").is_none());
    assert!(parse_parts_manifest("No table here").is_none());
}
