use agent_rs::agent_loop::tool_access::{ToolAccesses, ToolFileAccessOperation};

#[test]
fn read_and_search_do_not_conflict() {
    let a = ToolAccesses::read_file("/tmp/foo.txt");
    let b = ToolAccesses::search_tree("/tmp");
    assert!(!ToolAccesses::conflict(&a, &b));
}

#[test]
fn write_conflicts_with_recursive_read_under_same_tree() {
    let a = ToolAccesses::write_tree("/tmp");
    let b = ToolAccesses::read_file("/tmp/foo.txt");
    assert!(ToolAccesses::conflict(&a, &b));
}

#[test]
fn all_conflicts_with_everything() {
    let a = ToolAccesses::all();
    let b = ToolAccesses::read_file("/tmp/foo.txt");
    assert!(ToolAccesses::conflict(&a, &b));
    assert!(ToolAccesses::conflict(&b, &a));
}
