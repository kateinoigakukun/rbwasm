use std::path::PathBuf;

use rbwasm::{build_cruby, toolchain::Toolchain, BuildResult, BuildSource, Workspace, CRubyBuildInput};
use rbwasm_test_support::init_workspace;

fn fakeruby() -> PathBuf {
    PathBuf::from(file!())
        .parent()
        .unwrap()
        .join("fakeruby")
        .canonicalize()
        .unwrap()
}

#[test]
fn test_build_cruby_cached() {
    env_logger::init();
    let fakeruby = fakeruby();
    init_workspace!();
    let workspace =
        Workspace::create(PathBuf::from(".rbwasm").canonicalize().unwrap(), true).unwrap();
    let toolchain = Toolchain {
        wasm_opt: PathBuf::from("fake-wasm-opt"),
        wasi_sdk: PathBuf::from("fake-wasi-sdk"),
    };
    let build_source = BuildSource::Dir { path: fakeruby };
    let rb_wasm_support = BuildResult {
        install_dir: "/install/prefix".into(),
        cached: true,
        prefix: "/prefix".into(),
    };
    let input = CRubyBuildInput {
        source: build_source,
        asyncify_stack_size: 0,
        enabled_extentions: vec![],
        extra_cc_args: &[],
    };

    let result = build_cruby(&workspace, &toolchain, &input, &rb_wasm_support).unwrap();
    assert_eq!(result.cached, false);
    let result = build_cruby(&workspace, &toolchain, &input, &rb_wasm_support).unwrap();
    assert_eq!(result.cached, true);
}
