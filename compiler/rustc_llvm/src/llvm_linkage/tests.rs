use super::is_monolithic;

#[test]
fn shared_component_libraries() {
    assert!(is_monolithic(["-lLLVM"]));
    assert!(is_monolithic(["-lLLVM-23-rust-1.99.0-beta"]));
    assert!(!is_monolithic(["-lLLVM", "-lLLVMX86CodeGen"]));
    assert!(!is_monolithic(["-lLLVM", "-lPolly"]));
    assert!(!is_monolithic(["-lLLVMCore", "-lLLVMPasses"]));
    assert!(!is_monolithic(["-lLLVMCore"]));
    assert!(!is_monolithic(["-lLLVM-unknown"]));
    assert!(!is_monolithic([] as [&str; 0]));
}
