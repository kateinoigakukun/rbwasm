use rbwasm::*;
use rbwasm_test_support::init_workspace;
use std::path::PathBuf;
extern crate rbwasm_test_support;

#[test]
fn test_build_cruby() {
    let space = init_workspace!();
    let workspace =
        Workspace::create(PathBuf::from(".rbwasm").canonicalize().unwrap(), true).unwrap();
    let toolchain =
        toolchain::install_build_toolchain(&workspace).expect("failed toolchain install");
    let ruby_source = BuildSource::GitHub {
        owner: String::from("kateinoigakukun"),
        repo: String::from("ruby"),
        git_ref: String::from("9bcc194dc3c12f017a41b6287f85b58f2c487bf8"),
    };
    build_cruby(
        &workspace,
        &toolchain,
        &CRubyBuildInput {
            source: ruby_source,
            asyncify_stack_size: 0,
            enabled_extentions: vec![],
            extra_cc_args: &[],
        },
    )
    .expect("failed build cruby");
    drop(space)
}
