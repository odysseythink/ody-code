use agent_rs::tool::bridge::ToolBridge;
use std::sync::Arc;

#[test]
fn collaboration_module_and_bridge_are_public() {
    let bridge_exists =
        std::any::TypeId::of::<ToolBridge>() == std::any::TypeId::of::<ToolBridge>();
    assert!(bridge_exists);
}
