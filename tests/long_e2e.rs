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
        git_ref: String::from("834e12525261d756da85b9b880dabe8407084902"),
    };
    let asyncify_stack_size = 1024;
    let rb_wasm_support_source = BuildSource::GitHub {
        owner: String::from("kateinoigakukun"),
        repo: String::from("rb-wasm-support"),
        git_ref: String::from("0.4.0"),
    };
    let rb_wasm_support = build_rb_wasm_support(
        &workspace,
        &toolchain,
        &RbWasmSupportBuildInput {
            source: rb_wasm_support_source,
            asyncify_stack_size,
        },
    )
    .expect("failed build rb-wasm-support");
    build_cruby(
        &workspace,
        &toolchain,
        &CRubyBuildInput {
            source: ruby_source,
            asyncify_stack_size: 0,
            enabled_extentions: vec![],
        },
        &rb_wasm_support,
    )
    .expect("failed build cruby");
    drop(space)
}
