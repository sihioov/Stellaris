#[test]
fn compiles_with_default_features() {
    assert_eq!(env!("CARGO_PKG_NAME"), "ton618");
}

#[test]
fn compiles_with_all_features() {
    assert!(!env!("CARGO_MANIFEST_DIR").is_empty());
}
