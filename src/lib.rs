mod github;
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::bail;
use siphasher::sip128::SipHasher13;

pub struct Workspace {
    dir: PathBuf,
}

impl Workspace {
    pub fn new(dir: PathBuf) -> Workspace {
        Workspace { dir }
    }
    fn build_dir(&self) -> PathBuf {
        self.dir.join("build")
    }
    fn downloads_dir(&self) -> PathBuf {
        self.dir.join("downloads")
    }
    fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }
}

#[derive(Debug, Hash)]
pub enum BuildSource {
    GitHub {
        owner: String,
        repo: String,
        git_ref: String,
    },
    Dir {
        path: PathBuf,
    },
}

pub struct Toolchain {
    wasm_opt: PathBuf,
    wasi_sdk: PathBuf,
    rb_wasm_support: PathBuf,
}

pub fn install_build_toolchain(workspace: &Workspace) -> anyhow::Result<Toolchain> {
    const WASI_SDK_RELEASE_TARBALL: &str = "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-14/wasi-sdk-14.0-macos.tar.gz";
    const WASI_SDK_VERSION: &str = "14.0";
    let wasi_sdk_dest = workspace
        .downloads_dir()
        .join(format!("wasi-sdk-{}", WASI_SDK_VERSION));
    if !wasi_sdk_dest.exists() {
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
        std::fs::create_dir_all(rb_wasm_support_dest.as_path())?;
        let mut tar_gz =
            reqwest::blocking::get(RB_WASM_SUPPORT_RELEASE_TARBALL)?.error_for_status()?;
        extract_tarball(&mut tar_gz, &rb_wasm_support_dest)?;
    }

    Ok(Toolchain {
        wasm_opt: which::which("wasm-opt")?,
        wasi_sdk: wasi_sdk_dest.canonicalize()?,
        rb_wasm_support: rb_wasm_support_dest.canonicalize()?,
    })
}

impl BuildSource {
    fn hashed_srcname(&self, name: &str) -> String {
        let mut hasher = SipHasher13::new();
        self.hash(&mut hasher);
        let result = hasher.finish();
        let hex = hex::encode(result.to_le_bytes());
        format!("{}-{}", name, hex)
    }
}

fn install_cruby_src<'a>(source: &'a BuildSource, build_dir: &'a Path) -> anyhow::Result<&'a Path> {
    match source {
        BuildSource::GitHub {
            owner,
            repo,
            git_ref,
        } => {
            if build_dir.exists() {
                return Ok(build_dir);
            }
            std::fs::create_dir_all(build_dir)?;
            static APP_USER_AGENT: &str =
                concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
            let tar_gz = github::repo_archive_download_link(&owner, &repo, &git_ref);
            let client = reqwest::blocking::Client::builder()
                .user_agent(APP_USER_AGENT)
                .build()?;
            let response = client.get(tar_gz).send()?;
            let mut tar_gz = response.error_for_status()?;

            let mut tar = Command::new("tar")
                .args(["xz", "--strip-components", "1"])
                .current_dir(build_dir)
                .stdin(Stdio::piped())
                .spawn()?;
            std::io::copy(&mut tar_gz, &mut tar.stdin.take().unwrap())?;
            return Ok(build_dir);
        }
        BuildSource::Dir { path } => return Ok(path),
    }
}
pub fn build_cruby(
    workspace: &Workspace,
    toolchain: &Toolchain,
    source: &BuildSource,
) -> anyhow::Result<()> {
    let hashed_name = source.hashed_srcname("ruby");
    let build_dir = workspace.build_dir().join(&hashed_name);
    let install_dir = workspace.cache_dir().join(&hashed_name);
    let src_dir = install_cruby_src(source, &build_dir)?;
    let autogen_sh = src_dir.join("autogen.sh");
    let status = Command::new(autogen_sh.as_path()).status()?;
    if !status.success() {
        bail!("{:?} failed", autogen_sh)
    }

    let wasi_sdk = toolchain.wasi_sdk.as_path().to_string_lossy();
    let rb_wasm_support = toolchain.rb_wasm_support.as_path().to_string_lossy();
    let configure = src_dir.join("configure").canonicalize()?;
    let default_enabled_extensions = [
        "bigdecimal",
        "cgi/escape",
        "continuation",
        "coverage",
        "date",
        "dbm",
        "digest/bubblebabble",
        "digest",
        "digest/md5",
        "digest/rmd160",
        "digest/sha1",
        "digest/sha2",
        "etc",
        "fcntl",
        "fiber",
        "gdbm",
        "json",
        "json/generator",
        "json/parser",
        "nkf",
        "objspace",
        "pathname",
        "psych",
        "racc/cparse",
        "rbconfig/sizeof",
        "ripper",
        "stringio",
        "strscan",
        "monitor",
    ];
    let configure_args = [
        "--host=wasm32-unknown-wasi",
        "--disable-install-doc",
        "--disable-jit-support",
        "--with-coroutine=asyncify",
        "--with-static-linked-ext",
    ];

    let toolchain_args = ["XLDFLAGS=-Xlinker --relocatable"];
    let ldflags = [
        format!("--sysroot={}/share/wasi-sysroot", wasi_sdk),
        format!("-L{}/share/wasi-sysroot/lib/wasm32-wasi", wasi_sdk),
        format!("-L{}/lib", rb_wasm_support),
        String::from("-lwasi-emulated-mman"),
        String::from("-lwasi-emulated-signal"),
        String::from("-lwasi-emulated-getpid"),
        String::from("-lwasi-emulated-process-clocks"),
        String::from("-lrb_wasm_support"),
        String::from("-Xlinker"),
        String::from("--features=mutable-globals"),
    ];
    let cflags = [
        format!("--sysroot={}/share/wasi-sysroot", wasi_sdk),
        format!("-I{}/include", rb_wasm_support),
        String::from("-D_WASI_EMULATED_SIGNAL"),
        String::from("-D_WASI_EMULATED_MMAN"),
        String::from("-D_WASI_EMULATED_GETPID"),
        String::from("-D_WASI_EMULATED_PROCESS_CLOCKS"),
        String::from("-DRB_WASM_SUPPORT_EMULATE_SETJMP"),
    ];
    let mut configure = Command::new(configure);
    configure.current_dir(&build_dir);
    configure.args(configure_args);
    configure.arg(format!(
        "--prefix={}",
        install_dir.as_path().to_string_lossy()
    ));
    configure.arg(format!(
        "--with-ext={}",
        default_enabled_extensions.join(",")
    ));
    configure.args(toolchain_args);

    configure.arg(format!("LDFLAGS={}", ldflags.join(" ")));
    configure.arg(format!("CFLAGS={}", cflags.join(" ")));

    configure.arg(format!("CC={}/bin/clang", wasi_sdk));
    configure.arg(format!("LD={}/bin/clang", wasi_sdk));
    configure.arg(format!("AR={}/bin/llvm-ar", wasi_sdk));
    configure.arg(format!("RANLIB={}/bin/llvm-ranlib", wasi_sdk));

    log::debug!("configure cruby: {:?}", configure);
    let status = configure.status()?;
    if !status.success() {
        bail!("configuration of cruby failed")
    }

    let status = Command::new("make")
        .current_dir(&build_dir)
        .arg("install")
        .status()?;
    if !status.success() {
        bail!("make of cruby failed")
    }
    Ok(())
}

fn extract_tarball<R: std::io::Read>(src: &mut R, dest: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut tar = Command::new("tar")
        .args(["xz", "--strip-components", "1"])
        .current_dir(dest)
        .stdin(Stdio::piped())
        .spawn()?;
    std::io::copy(src, &mut tar.stdin.take().unwrap())?;
    Ok(())
}
