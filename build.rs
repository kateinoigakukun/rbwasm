use std::{env::var_os, path::Path, process::Command};

fn main() {
    let out_dir = var_os("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    build_libwasi_vfs_a(out_dir);
}

fn build_libwasi_vfs_a(out_dir: &Path) {
    let src = Path::new("./wasi-vfs");
    let target_dir = out_dir.join("wasi-vfs-target");
    std::fs::create_dir_all(&target_dir).unwrap();
    let target_dir = target_dir.canonicalize().unwrap();
    let status = Command::new("cargo")
        .current_dir(src)
        .args(["build", "--target", "wasm32-unknown-unknown", "--release"])
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .unwrap();
    if !status.success() {
        eprintln!("Failed building libwasi_vfs.a: {}", status);
        std::process::exit(-1);
    }
}
