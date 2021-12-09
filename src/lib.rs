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
use regex::Regex;
use siphasher::sip128::SipHasher13;

use crate::toolchain::Toolchain;
use crate::ui::trace_command_exec;

pub struct Workspace {
    dir: PathBuf,
    save_temps: bool,
    tempfile_owner: Vec<tempfile::NamedTempFile>,
}

impl Workspace {
    pub fn create(dir: PathBuf, save_temps: bool) -> std::io::Result<Workspace> {
        let space = Workspace {
            dir,
            save_temps,
            tempfile_owner: vec![],
        };
        std::fs::create_dir_all(space.build_dir())?;
        std::fs::create_dir_all(space.downloads_dir())?;
        std::fs::create_dir_all(space.cache_dir())?;
        std::fs::create_dir_all(space.temporary_dir())?;
        Ok(space)
    }

    /// Note that caller can assume the returned directory exists
    fn build_dir(&self) -> PathBuf {
        self.dir.join("build")
    }
    /// Note that caller can assume the returned directory exists
    fn downloads_dir(&self) -> PathBuf {
        self.dir.join("downloads")
    }
    /// Note that caller can assume the returned directory exists
    fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }
    /// Note that caller can assume the returned directory exists
    fn temporary_dir(&self) -> PathBuf {
        self.dir.join("tmp")
    }

    fn with_overriding_command<R, F: FnOnce(PathBuf) -> R>(
        &self,
        cmd: &str,
        inner: F,
    ) -> anyhow::Result<R> {
        let fake_bin_dir = tempfile::tempdir_in(self.temporary_dir())?;
        let fake_bin_dir_path = fake_bin_dir.path().to_path_buf();
        let fake_bin = fake_bin_dir_path.join(cmd);
        {
            let mut fake_bin = File::create(fake_bin)?;
            let true_bin =
                which::which("true").with_context(|| format!("true command not found"))?;
            fake_bin.write_all(format!("#!{}\n", true_bin.to_string_lossy()).as_bytes())?;
            let mut perm = fake_bin.metadata()?.permissions();
            // chmod +x
            perm.set_mode(perm.mode() | 0o111);
            fake_bin.set_permissions(perm)?;
        }

        let result = inner(fake_bin_dir_path);
        if self.save_temps {
            std::mem::forget(fake_bin_dir);
        }

        Ok(result)
    }

    pub fn tempfile<F: FnOnce(&mut tempfile::NamedTempFile) -> anyhow::Result<()>>(
        &mut self,
        prefix: &str,
        inner: F,
    ) -> anyhow::Result<PathBuf> {
        let mut tmpfile = tempfile::Builder::new()
            .prefix(prefix)
            .tempfile_in(self.temporary_dir())?;
        let tmpfile_path = tmpfile.path().to_path_buf();
        inner(&mut tmpfile)?;
        if self.save_temps {
            tmpfile.keep()?;
        } else {
            self.tempfile_owner.push(tmpfile);
        }

        Ok(tmpfile_path)
    }

    fn hashed_dirs<T: Hash>(&self, source: T, name: &str) -> (PathBuf, PathBuf) {
        let mut hasher = SipHasher13::new();
        source.hash(&mut hasher);
        let result = hasher.finish();
        let hex = hex::encode(result.to_le_bytes());
        let hashed = format!("{}-{}", name, hex);
        let build_dir = self.build_dir().join(&hashed);
        let install_dir = self.cache_dir().join(&hashed);
        (build_dir, install_dir)
    }
}

pub struct BuildResult {
    pub install_dir: PathBuf,
    pub cached: bool,
    pub prefix: PathBuf,
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

/// Retrieve a build source from BuildSource and returns source directory
fn install_build_src<'a>(source: &'a BuildSource, build_dir: &'a Path) -> anyhow::Result<&'a Path> {
    match source {
        BuildSource::GitHub {
            owner,
            repo,
            git_ref,
        } => {
            if build_dir.exists() {
                return Ok(build_dir);
            }
            ui_info!(
                "downloading {}/{} source into {:?}",
                owner,
                repo,
                relpath_for_display(build_dir),
            );
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

pub const DEFAULT_ENABLED_EXTENSIONS: [&str; 29] = [
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

fn configure_cruby(
    toolchain: &Toolchain,
    src_dir: &Path,
    build_dir: &Path,
    install_dir: &Path,
    prefix: &Path,
    asyncify_stack_size: usize,
    enabled_extensions: Vec<&str>,
    extra_cc_args: &[String],
) -> anyhow::Result<()> {
    log::info!("configure cruby");
    let wasi_sdk = toolchain.wasi_sdk.as_path().to_string_lossy();

    std::fs::create_dir_all(build_dir).with_context(|| format!("failed to create build dir"))?;

    let configure = src_dir.join("configure").canonicalize()?;
    let ldflags = [
        format!("--sysroot={}/share/wasi-sysroot", wasi_sdk),
        format!("-L{}/share/wasi-sysroot/lib/wasm32-wasi", wasi_sdk),
        String::from("-lwasi-emulated-mman"),
        String::from("-lwasi-emulated-signal"),
        String::from("-lwasi-emulated-getpid"),
        String::from("-lwasi-emulated-process-clocks"),
        String::from("-Xlinker"),
        String::from("--features=mutable-globals"),
    ];
    let mut cflags = vec![
        format!("--sysroot={}/share/wasi-sysroot", wasi_sdk),
        String::from("-D_WASI_EMULATED_SIGNAL"),
        String::from("-D_WASI_EMULATED_MMAN"),
        String::from("-D_WASI_EMULATED_GETPID"),
        String::from("-D_WASI_EMULATED_PROCESS_CLOCKS"),
        String::from("-DRB_WASM_SUPPORT_EMULATE_SETJMP"),
        format!(
            "-DRB_WASM_SUPPORT_FRAME_BUFFER_SIZE={}",
            asyncify_stack_size
        ),
    ];
    cflags.extend(extra_cc_args.to_vec());
    if let Ok(total_size) = std::env::var("TRANSIENT_HEAP_TOTAL_SIZE") {
        cflags.push(format!("-DTRANSIENT_HEAP_TOTAL_SIZE={}", total_size));
    }
    let mut configure_cmd = Command::new(configure.as_path());
    configure_cmd.current_dir(&build_dir);

    if !is_debugging() {
        configure_cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }

    configure_cmd.args([
        "--host=wasm32-unknown-wasi",
        "--disable-install-doc",
        "--disable-jit-support",
        "--with-coroutine=asyncify",
        "--with-static-linked-ext",
    ]);
    configure_cmd.arg(format!("--prefix={}", prefix.to_string_lossy()));
    configure_cmd.arg(format!("--with-destdir={}", install_dir.to_string_lossy()));
    configure_cmd.arg(format!("--with-ext={}", enabled_extensions.join(",")));
    configure_cmd.arg("XLDFLAGS=-Xlinker --relocatable");
    configure_cmd.arg(format!("LDFLAGS={}", ldflags.join(" ")));
    configure_cmd.arg(format!("CFLAGS={}", cflags.join(" ")));
    configure_cmd.arg(format!("CC={}/bin/clang", wasi_sdk));
    configure_cmd.arg(format!("LD={}/bin/clang", wasi_sdk));
    configure_cmd.arg(format!("AR={}/bin/llvm-ar", wasi_sdk));
    configure_cmd.arg(format!("RANLIB={}/bin/llvm-ranlib", wasi_sdk));

    trace_command_exec(&configure_cmd, "./configure", Some(&build_dir));
    let status = configure_cmd
        .status()
        .with_context(|| format!("failed to spawn {:?}", configure))?;
    if !status.success() {
        bail!("configuration of cruby failed")
    }
    Ok(())
}

#[derive(Hash)]
pub struct CRubyBuildInput<'a> {
    pub source: BuildSource,
    pub asyncify_stack_size: usize,
    pub extra_cc_args: &'a [String],
    pub enabled_extentions: Vec<&'a str>,
}

/// Build CRuby from a given source and returns installed path
pub fn build_cruby(
    workspace: &Workspace,
    toolchain: &Toolchain,
    input: &CRubyBuildInput,
) -> anyhow::Result<BuildResult> {
    log::info!("build cruby...");
    const GUEST_RUBY_ROOT: &str = "/embd-root/ruby";
    let guest_ruby_root: PathBuf = GUEST_RUBY_ROOT.into();
    let (build_dir, install_dir) = workspace.hashed_dirs(input, "ruby");
    if install_dir.exists() {
        log::info!("cruby build cache found. skip building again");
        return Ok(BuildResult {
            install_dir,
            cached: true,
            prefix: guest_ruby_root,
        });
    }

    let src_dir = install_build_src(&input.source, &build_dir)?;
    let autogen_sh = src_dir.join("autogen.sh");
    let mut autogen_sh = Command::new(autogen_sh.as_path());
    trace_command_exec(&autogen_sh, "./autogen.sh", None);

    let status = autogen_sh
        .status()
        .with_context(|| format!("failed to spawn {:?}", autogen_sh))?;
    if !status.success() {
        bail!("{:?} failed", autogen_sh)
    }

    configure_cruby(
        toolchain,
        src_dir,
        &build_dir,
        &install_dir,
        &guest_ruby_root,
        input.asyncify_stack_size,
        input.enabled_extentions.clone(),
        input.extra_cc_args,
    )
    .with_context(|| format!("configuration failed"))?;

    let status: anyhow::Result<ExitStatus> =
        // wasm-opt doesn't support relocatable input but clang always apply wasm-opt whenever it's installed.
        // However rbwasm uses --relocatable linker flag to concatenate all object files including native exts
        // into single object file and link vfs object file after building CRuby.
        // Therefore, override wasm-opt with fake binary to avoid breaking reloc section produced by --relocatable
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
                .arg("install")
                .arg(format!("-j{}", num_cpus::get()));

            if !is_debugging() {
                make.stdout(Stdio::null()).stderr(Stdio::null());
            }
            trace_command_exec(&make, "make install", Some(&build_dir));
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
        prefix: guest_ruby_root,
    })
}

pub struct LinkerInput<'a> {
    pub stack_size: usize,
    pub raw_objects: Vec<(String, Vec<u8>)>,
    pub extra_args: &'a [String],
}

pub fn link_executable(
    workspace: &mut Workspace,
    toolchain: &Toolchain,
    cruby: &BuildResult,
    input: &LinkerInput,
    output: &Path,
) -> anyhow::Result<()> {
    log::info!("link single ruby binary");
    let wasm_ld = toolchain.wasi_sdk.join("bin/wasm-ld");
    let mut link = Command::new(wasm_ld);
    link.arg(
        cruby
            .install_dir
            .join(cruby.prefix.strip_prefix("/")?)
            .join("bin/ruby"),
    );
    link.args(["--stack-first", "-z"]);
    link.arg(format!("stack-size={}", input.stack_size));
    link.arg("-o");
    link.arg(output);
    link.args(input.extra_args);

    fn link_inner(mut link: Command, workspace: &mut Workspace) -> anyhow::Result<ExitStatus> {
        let libvfs_path = workspace.tempfile("libwasi_vfs.a", |libvfs| {
            libvfs.write_all(std::include_bytes!(std::concat!(
                std::env!("OUT_DIR"),
                "/wasi-vfs-target/wasm32-unknown-unknown/release/libwasi_vfs.a"
            )))?;
            Ok(())
        })?;

        link.arg(libvfs_path);
        trace_command_exec(&link, "linker", None);
        let status = link
            .status()
            .with_context(|| format!("failed to spawn linker"))?;
        Ok(status)
    }

    for (prefix, bytes) in &input.raw_objects {
        let objfile_path = workspace.tempfile(&prefix, |objfile| {
            objfile.write_all(bytes)?;
            Ok(())
        })?;
        link.arg(objfile_path);
    }
    let status = link_inner(link, workspace)?;

    if !status.success() {
        bail!("link failed")
    }
    Ok(())
}

pub fn asyncify_executable(
    toolchain: &Toolchain,
    with_debuginfo: bool,
    input: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    log::info!("asyncify ruby binary");
    let mut wasm_opt = Command::new(&toolchain.wasm_opt);
    wasm_opt.arg(&input);
    wasm_opt.arg("--asyncify");
    wasm_opt.arg("-O");
    if with_debuginfo {
        wasm_opt.arg("-g");
    }
    wasm_opt.arg("--pass-arg=asyncify-ignore-imports");
    wasm_opt.arg("-o");
    wasm_opt.arg(&output);
    trace_command_exec(&wasm_opt, "asyncify", None);
    let status = wasm_opt
        .status()
        .with_context(|| format!("failed to spawn wasm-opt"))?;
    if !status.success() {
        bail!("wasm-opt failed")
    }
    Ok(())
}

pub struct MkfsInput<'a> {
    pub host_ruby_root: &'a Path,
    pub guest_ruby_root: &'a Path,
    pub map_paths: Vec<(PathBuf, PathBuf)>,
}

fn expand_map_dir(
    map_dir: (PathBuf, PathBuf),
    host_ruby_root: &Path,
    guest_ruby_root: &Path,
) -> (PathBuf, PathBuf) {
    let (mut guest, mut host) = map_dir;
    let magic_prefix = "@ruby_root";
    if let Ok(stripped) = host.strip_prefix(magic_prefix) {
        host = host_ruby_root.join(stripped);
    }
    if let Ok(stripped) = guest.strip_prefix(magic_prefix) {
        guest = guest_ruby_root.join(stripped);
    }
    (guest, host)
}

pub fn builtin_map_paths(installed_ruby_root: &Path) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    log::info!("collect builtin files to map them in vfs");
    let excludes = [
        ".*/cache/.*\\.gem$",
        ".*/libruby-static\\.a$",
        ".*/bin/ruby$",
    ];
    let excludes = excludes
        .into_iter()
        .map(Regex::new)
        .collect::<Result<Vec<_>, regex::Error>>()?;

    fn visit_dirs(dir: &Path, excludes: &[Regex], paths: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        for entry in
            std::fs::read_dir(dir).with_context(|| format!("failed to read dir: {:?}", dir))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, excludes, paths)?;
            } else {
                let path_str = path.to_string_lossy();
                let is_excluded = excludes.iter().any(|x| x.is_match(&path_str));
                if !is_excluded {
                    paths.push(path);
                } else {
                    log::debug!("vfs: excluded {:?}", path);
                }
            }
        }
        Ok(())
    }
    let mut paths = vec![];
    visit_dirs(installed_ruby_root, &excludes, &mut paths)?;
    Ok(paths
        .into_iter()
        .map(move |path| {
            let guest = path.strip_prefix(installed_ruby_root).unwrap();
            let guest = Path::new("@ruby_root").join(guest);
            let host = path;
            (guest, host)
        })
        .collect())
}

pub fn mkfs(
    workspace: &Workspace,
    toolchain: &Toolchain,
    input: MkfsInput,
) -> anyhow::Result<Vec<u8>> {
    ui_info!("generating vfs image");
    let map_paths = input
        .map_paths
        .into_iter()
        .map(|map| expand_map_dir(map, input.host_ruby_root, input.guest_ruby_root));
    let fs_c_src = wasi_vfs_mkfs::generate_c_source(map_paths)?;
    if is_debugging() {
        let fs_c = workspace.temporary_dir().join("fs.c");
        ui_info!("exporting vfs intermediate source to {:?}", &fs_c);
        if let Err(e) = std::fs::write(&fs_c, &fs_c_src) {
            log::warn!(
                "failed to export vfs intermediate source into {:?}: {}",
                relpath_for_display(&fs_c),
                e
            );
        }
    }
    let clang = toolchain.wasi_sdk.join("bin/clang");
    let object = wasi_vfs_mkfs::generate_obj(&fs_c_src, &clang.to_string_lossy())?;
    Ok(object)
}

pub fn mkargs(
    workspace: &Workspace,
    toolchain: &Toolchain,
    args: &[String],
) -> anyhow::Result<Vec<u8>> {
    ui_info!("generating preset arguments data");
    let preset_args_c_src = wasi_preset_args::generate_c_source("ruby.wasm", args)?;
    if is_debugging() {
        let preset_args_c = workspace.temporary_dir().join("preset-args.c");
        ui_info!(
            "exporting preset args intermediate source to {:?}",
            &preset_args_c
        );
        if let Err(e) = std::fs::write(&preset_args_c, &preset_args_c_src) {
            log::warn!(
                "failed to export preset args intermediate source into {:?}: {}",
                relpath_for_display(&preset_args_c),
                e
            );
        }
    }
    let clang = toolchain.wasi_sdk.join("bin/clang");
    let object = wasi_preset_args::generate_obj(&preset_args_c_src, &clang.to_string_lossy())?;
    Ok(object)
}

pub fn run_build_hook(build_hook: &str, host_ruby_root: &Path) -> anyhow::Result<()> {
    ui_info!("running build-hook {}", build_hook);
    let mut hook = Command::new(&build_hook)
        .env("RUBY_ROOT", host_ruby_root)
        .spawn()
        .with_context(|| format!("failed to spawn build hook: {}", build_hook))?;
    hook.try_wait()
        .with_context(|| format!("error while build hook"))?;
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

fn relpath_for_display(path: &Path) -> &Path {
    if let Ok(cwd) = std::env::current_dir() {
        path.strip_prefix(cwd).unwrap_or(path)
    } else {
        path
    }
}

pub(crate) fn is_debugging() -> bool {
    std::env::var("RBWASM_DEBUG").is_ok()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::expand_map_dir;

    #[test]
    fn test_expand_map_dir() {
        let (guest, host) = expand_map_dir(
            ("/gems".into(), "@ruby_root/lib/gems".into()),
            Path::new("/install/prefix"),
            Path::new("/prefix"),
        );
        assert_eq!(host.to_string_lossy(), "/install/prefix/lib/gems");
        assert_eq!(guest.to_string_lossy(), "/gems");
    }
}
