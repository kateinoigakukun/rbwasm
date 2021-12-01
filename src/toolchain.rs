use std::path::PathBuf;

use anyhow::Context;

use crate::{extract_tarball, ui_info, Workspace};

pub struct Toolchain {
    pub wasm_opt: PathBuf,
    pub wasi_sdk: PathBuf,
    pub rb_wasm_support: PathBuf,
}

pub fn install_build_toolchain(workspace: &Workspace) -> anyhow::Result<Toolchain> {
    log::info!("install build toolchain...");
    const WASI_SDK_RELEASE_TARBALL: &str = "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-14/wasi-sdk-14.0-macos.tar.gz";
    const WASI_SDK_VERSION: &str = "14.0";
    let wasi_sdk_dest = workspace
        .downloads_dir()
        .join(format!("wasi-sdk-{}", WASI_SDK_VERSION));
    if !wasi_sdk_dest.exists() {
        ui_info!(
            "installing wasi-sdk {} into {:?}",
            WASI_SDK_VERSION,
            &wasi_sdk_dest
        );
        std::fs::create_dir_all(wasi_sdk_dest.as_path())?;
        let mut tar_gz = reqwest::blocking::get(WASI_SDK_RELEASE_TARBALL)?.error_for_status()?;
        extract_tarball(&mut tar_gz, &wasi_sdk_dest)?;
    }

    const RB_WASM_SUPPORT_RELEASE_TARBALL: &str = "https://github.com/kateinoigakukun/rb-wasm-support/releases/download/0.4.0/rb-wasm-support-wasm32-unknown-wasi.tar.gz";
    const RB_WASM_SUPPORT_VERSION: &str = "0.4.0";
    let rb_wasm_support_dest = workspace
        .downloads_dir()
        .join(format!("rb-wasm-support-{}", RB_WASM_SUPPORT_VERSION));

    if !rb_wasm_support_dest.exists() {
        ui_info!(
            "installing rb-wasm-support {} into {:?}",
            RB_WASM_SUPPORT_VERSION,
            &rb_wasm_support_dest
        );
        std::fs::create_dir_all(rb_wasm_support_dest.as_path())?;
        let mut tar_gz =
            reqwest::blocking::get(RB_WASM_SUPPORT_RELEASE_TARBALL)?.error_for_status()?;
        extract_tarball(&mut tar_gz, &rb_wasm_support_dest)?;
    }

    Ok(Toolchain {
        wasm_opt: which::which("wasm-opt")
            .with_context(|| format!("wasm-opt command not found"))?,
        wasi_sdk: wasi_sdk_dest.canonicalize()?,
        rb_wasm_support: rb_wasm_support_dest.canonicalize()?,
    })
}
