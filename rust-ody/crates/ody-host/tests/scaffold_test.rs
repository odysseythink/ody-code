#[test]
fn ody_host_binary_exists() {
    assert_eq!(env!("CARGO_PKG_NAME"), "ody-host");
}
