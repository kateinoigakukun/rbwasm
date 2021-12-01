use std::path::PathBuf;

use rbwasm::{build_cruby, toolchain::Toolchain, BuildSource, Workspace};
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
    let workspace = Workspace::new(PathBuf::from(".rbwasm").canonicalize().unwrap(), true);
    let toolchain = Toolchain {
        wasm_opt: PathBuf::from("fake-wasm-opt"),
        wasi_sdk: PathBuf::from("fake-wasi-sdk"),
        rb_wasm_support: PathBuf::from("fake-rb-wasm-support"),
    };
    let build_source = BuildSource::Dir { path: fakeruby };

    let result = build_cruby(&workspace, &toolchain, &build_source).unwrap();
    assert_eq!(result.cached, false);
    let result = build_cruby(&workspace, &toolchain, &build_source).unwrap();
    assert_eq!(result.cached, true);
}
