//! Test to verify test infrastructure works correctly

mod common;

use common::builders::VariableBuilder;
use datavis_rs::VariableType;

#[test]
fn test_infrastructure_setup() {
    // Test that builders work
    let var = VariableBuilder::new("test_var")
        .address(0x20000100)
        .var_type(VariableType::F32)
        .build();

    assert_eq!(var.name, "test_var");
    assert_eq!(var.address, 0x20000100);
    assert_eq!(var.var_type, VariableType::F32);
}

#[test]
fn test_float_comparison() {
    common::assert_float_eq(1.0, 1.0000001, 0.001);
}

#[test]
#[should_panic]
fn test_float_comparison_fails() {
    common::assert_float_eq(1.0, 2.0, 0.001);
}
