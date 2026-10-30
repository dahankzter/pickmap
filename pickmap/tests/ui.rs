#[test]
#[cfg_attr(miri, ignore = "trybuild runs rustc")]
fn compile_errors() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
