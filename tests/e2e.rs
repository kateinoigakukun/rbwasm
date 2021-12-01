use rbwasm::*;
use rbwasm_test_support::init_workspace;
use std::path::PathBuf;
extern crate rbwasm_test_support;

#[test]
fn test_build_cruby() {
    let space = init_workspace!();
    let workspace = Workspace::new(PathBuf::from(".rbwasm").canonicalize().unwrap(), true);
    let toolchain = toolchain::install_build_toolchain(&workspace).expect("failed toolchain install");
    let ruby_source = BuildSource::GitHub {
        owner: String::from("kateinoigakukun"),
        repo: String::from("ruby"),
        git_ref: String::from("834e12525261d756da85b9b880dabe8407084902"),
    };
    build_cruby(&workspace, &toolchain, &ruby_source).expect("failed build cruby");
    drop(space)
}
