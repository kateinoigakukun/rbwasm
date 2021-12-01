mod github;
pub mod toolchain;
mod ui;
use std::{
    fs::File,
    hash::{Hash, Hasher},
    io::Write,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

use anyhow::{bail, Context};
use siphasher::sip128::SipHasher13;

use crate::toolchain::Toolchain;
use crate::ui::trace_command_exec;

pub struct Workspace {
    dir: PathBuf,
    save_temps: bool,
}

impl Workspace {
    pub fn new(dir: PathBuf, save_temps: bool) -> Workspace {
        Workspace { dir, save_temps }
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
    fn temporary_dir(&self) -> PathBuf {
        self.dir.join("tmp")
    }

    fn with_overriding_command<R, F: FnOnce(PathBuf) -> R>(
        &self,
        cmd: &str,
        inner: F,
    ) -> anyhow::Result<R> {
        std::fs::create_dir_all(self.temporary_dir())?;
        let fake_bin_dir = tempfile::tempdir_in(self.temporary_dir())?;
        let fake_bin_dir_path = fake_bin_dir.path().to_path_buf();
        let fake_bin = fake_bin_dir_path.join(cmd);
        let mut fake_bin = File::create(fake_bin)?;
        let true_bin = which::which("true").with_context(|| format!("true command not found"))?;
        fake_bin.write_all(format!("#!{}\n", true_bin.to_string_lossy()).as_bytes())?;
        let mut perm = fake_bin.metadata()?.permissions();
        // chmod +x
        perm.set_mode(perm.mode() | 0o111);
        fake_bin.set_permissions(perm)?;
        let result = inner(fake_bin_dir_path);
        if self.save_temps {
            std::mem::forget(fake_bin_dir);
        }

        Ok(result)
    }

    pub fn with_tempfile<R, F: FnOnce(&mut File, PathBuf) -> R>(
        &self,
        prefix: &str,
        inner: F,
    ) -> anyhow::Result<R> {
        std::fs::create_dir_all(self.temporary_dir())?;
        let mut tmpfile = tempfile::Builder::new()
            .prefix(prefix)
            .tempfile_in(self.temporary_dir())?;
        let tmpfile_path = tmpfile.path().to_path_buf();

        let result = inner(tmpfile.as_file_mut(), tmpfile_path);

        if self.save_temps {
            tmpfile.keep()?;
        }

        Ok(result)
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

impl BuildSource {
    fn hashed_srcname(&self, name: &str) -> String {
        let mut hasher = SipHasher13::new();
        self.hash(&mut hasher);
        let result = hasher.finish();
        let hex = hex::encode(result.to_le_bytes());
        format!("{}-{}", name, hex)
    }
}

/// Retrieve a CRuby source from BuildSource and returns source directory
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

fn configure_cruby(
    toolchain: &Toolchain,
    src_dir: &Path,
    build_dir: &Path,
    install_dir: &Path,
) -> anyhow::Result<()> {
    log::info!("configure cruby");
    let wasi_sdk = toolchain.wasi_sdk.as_path().to_string_lossy();
    let rb_wasm_support = toolchain.rb_wasm_support.as_path().to_string_lossy();

    std::fs::create_dir_all(build_dir).with_context(|| format!("failed to create build dir"))?;

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
    let configure = src_dir.join("configure").canonicalize()?;
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
    let mut configure_cmd = Command::new(configure.as_path());
    configure_cmd
        .current_dir(&build_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_cmd.args([
        "--host=wasm32-unknown-wasi",
        "--disable-install-doc",
        "--disable-jit-support",
        "--with-coroutine=asyncify",
        "--with-static-linked-ext",
    ]);
    configure_cmd.arg(format!("--prefix={}", install_dir.to_string_lossy()));
    configure_cmd.arg(format!(
        "--with-ext={}",
        default_enabled_extensions.join(",")
    ));
    configure_cmd.arg("XLDFLAGS=-Xlinker --relocatable");
    configure_cmd.arg(format!("LDFLAGS={}", ldflags.join(" ")));
    configure_cmd.arg(format!("CFLAGS={}", cflags.join(" ")));
    configure_cmd.arg(format!("CC={}/bin/clang", wasi_sdk));
    configure_cmd.arg(format!("LD={}/bin/clang", wasi_sdk));
    configure_cmd.arg(format!("AR={}/bin/llvm-ar", wasi_sdk));
    configure_cmd.arg(format!("RANLIB={}/bin/llvm-ranlib", wasi_sdk));

    trace_command_exec(&configure_cmd, Some(&build_dir));
    let status = configure_cmd
        .status()
        .with_context(|| format!("failed to spawn {:?}", configure))?;
    if !status.success() {
        bail!("configuration of cruby failed")
    }
    Ok(())
}

pub struct BuildResult {
    pub install_dir: PathBuf,
    pub cached: bool,
}

/// Build CRuby from a given source and returns installed path
pub fn build_cruby(
    workspace: &Workspace,
    toolchain: &Toolchain,
    source: &BuildSource,
) -> anyhow::Result<BuildResult> {
    log::info!("build cruby...");
    let hashed_name = source.hashed_srcname("ruby");
    let build_dir = workspace.build_dir().join(&hashed_name);
    let install_dir = workspace.cache_dir().join(&hashed_name);
    if install_dir.exists() {
        log::info!("cruby build cache found. skip building again");
        return Ok(BuildResult {
            install_dir,
            cached: true,
        });
    }

    let src_dir = install_cruby_src(source, &build_dir)?;
    let autogen_sh = src_dir.join("autogen.sh");
    let mut autogen_sh = Command::new(autogen_sh.as_path());
    trace_command_exec(&autogen_sh, None);

    let status = autogen_sh
        .status()
        .with_context(|| format!("failed to spawn {:?}", autogen_sh))?;
    if !status.success() {
        bail!("{:?} failed", autogen_sh)
    }

    configure_cruby(toolchain, src_dir, &build_dir, &install_dir)
        .with_context(|| format!("configuration failed"))?;

    let status: anyhow::Result<ExitStatus> =
        workspace.with_overriding_command("wasm-opt", |fake_path| {
            let new_path = if let Some(current_path) = std::env::var_os("PATH") {
                let mut current_paths = std::env::split_paths(&current_path).collect::<Vec<_>>();
                current_paths.insert(0, fake_path.to_path_buf());
                std::env::join_paths(&current_paths).with_context(|| {
                    format!("failed to join PATh with {}", fake_path.to_string_lossy())
                })?
            } else {
                fake_path.as_os_str().to_os_string()
            };
            let mut make = Command::new("make");
            log::info!("setting PATH='{}'", new_path.to_string_lossy());
            make.current_dir(&build_dir)
                .env("PATH", new_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .arg("install")
                .arg(format!("-j{}", num_cpus::get()));
            trace_command_exec(&make, Some(&build_dir));
            let status = make
                .status()
                .with_context(|| format!("failed to spawn make"))?;
            Ok(status)
        })?;
    let status = status?;
    if !status.success() {
        bail!("make of cruby failed")
    }
    Ok(BuildResult {
        install_dir,
        cached: false,
    })
}

pub struct LinkerInput<'a> {
    pub stack_size: usize,
    pub fs_object: Option<&'a [u8]>,
}

pub fn link_executable(
    workspace: &Workspace,
    toolchain: &Toolchain,
    cruby: &BuildResult,
    input: &LinkerInput,
    output: &Path,
) -> anyhow::Result<()> {
    log::info!("link single ruby binary");
    let wasm_ld = toolchain.wasi_sdk.join("bin/wasm-ld");
    let mut link = Command::new(wasm_ld);
    link.arg(cruby.install_dir.join("bin/ruby"));
    link.args(["--stack-first", "-z"]);
    link.arg(format!("stack-size={}", input.stack_size));
    link.arg("-o");
    link.arg(output);

    fn link_inner(mut link: Command, workspace: &Workspace) -> anyhow::Result<ExitStatus> {
        workspace.with_tempfile("libwasi_vfs.a", |libvfs, libvfs_path| {
            libvfs.write_all(std::include_bytes!(std::concat!(
                std::env!("OUT_DIR"),
                "/wasi-vfs-target/wasm32-unknown-unknown/release/libwasi_vfs.a"
            )))?;

            link.arg(libvfs_path);
            trace_command_exec(&link, None);
            let status = link
                .status()
                .with_context(|| format!("failed to spawn linker"))?;
            Ok(status)
        })?
    }

    let status = if let Some(bytes) = input.fs_object {
        let status = workspace.with_tempfile("fs.o", |fs_obj, fs_obj_path| {
            fs_obj.write_all(&bytes)?;
            link.arg(fs_obj_path);
            link_inner(link, &workspace)
        })?;
        status?
    } else {
        link_inner(link, &workspace)?
    };

    if !status.success() {
        bail!("link failed")
    }
    Ok(())
}

pub fn asyncify_executable(
    toolchain: &Toolchain,
    input: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    log::info!("asyncify ruby binary");
    let mut wasm_opt = Command::new(&toolchain.wasm_opt);
    wasm_opt.arg(&input);
    wasm_opt.arg("--asyncify");
    wasm_opt.arg("-O");
    wasm_opt.arg("--pass-arg=asyncify-ignore-imports");
    wasm_opt.arg("-o");
    wasm_opt.arg(&output);
    trace_command_exec(&wasm_opt, None);
    let status = wasm_opt
        .status()
        .with_context(|| format!("failed to spawn wasm-opt"))?;
    if !status.success() {
        bail!("wasm-opt failed")
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
