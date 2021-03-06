use std::path::PathBuf;

use anyhow::Context;

use crate::{extract_tarball, relpath_for_display, ui_info, Workspace};

pub struct Toolchain {
    pub wasm_opt: PathBuf,
    pub wasi_sdk: PathBuf,
}

pub fn install_build_toolchain(workspace: &Workspace) -> anyhow::Result<Toolchain> {
    log::info!("install build toolchain...");
    #[cfg(target_os = "macos")]
    const WASI_SDK_RELEASE_TARBALL: &str = "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-14/wasi-sdk-14.0-macos.tar.gz";
    #[cfg(target_os = "linux")]
    const WASI_SDK_RELEASE_TARBALL: &str = "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-14/wasi-sdk-14.0-linux.tar.gz";
    #[cfg(target_os = "windows")]
    const WASI_SDK_RELEASE_TARBALL: &str = "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-14/wasi-sdk-14.0-mingw.tar.gz";

    const WASI_SDK_VERSION: &str = "14.0";
    let wasi_sdk_dest = workspace
        .downloads_dir()
        .join(format!("wasi-sdk-{}", WASI_SDK_VERSION));
    if !wasi_sdk_dest.exists() {
        ui_info!(
            "installing wasi-sdk {} into {:?}",
            WASI_SDK_VERSION,
            relpath_for_display(&wasi_sdk_dest)
        );
        std::fs::create_dir_all(wasi_sdk_dest.as_path())?;
        let mut tar_gz = reqwest::blocking::get(WASI_SDK_RELEASE_TARBALL)?.error_for_status()?;
        extract_tarball(&mut tar_gz, &wasi_sdk_dest)?;
    }

    Ok(Toolchain {
        wasm_opt: which::which("wasm-opt")
            .with_context(|| format!("wasm-opt command not found"))?,
        wasi_sdk: wasi_sdk_dest.canonicalize()?,
    })
}
