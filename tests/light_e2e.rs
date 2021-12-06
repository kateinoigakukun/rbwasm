use std::path::PathBuf;

use rbwasm::{build_cruby, toolchain::Toolchain, BuildResult, BuildSource, Workspace};
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

    let result = build_cruby(&workspace, &toolchain, &build_source, &rb_wasm_support, 0, vec![]).unwrap();
    assert_eq!(result.cached, false);
    let result = build_cruby(&workspace, &toolchain, &build_source, &rb_wasm_support, 0, vec![]).unwrap();
    assert_eq!(result.cached, true);
}
