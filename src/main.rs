use anyhow::{bail, Context};
use rbwasm::{build_cruby, install_build_toolchain, BuildSource, Workspace};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, ExitStatus},
};
use structopt::StructOpt;

fn parse_map_dirs(s: &str) -> anyhow::Result<(String, String)> {
    let parts: Vec<&str> = s.split("::").collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "must contain exactly one double colon ('::')"
        ));
    }
    Ok((parts[0].into(), parts[1].into()))
}

#[derive(StructOpt)]
struct Opt {
    #[structopt(long = "mapdir", number_of_values = 1, value_name = "GUEST_DIR::HOST_DIR", parse(try_from_str = parse_map_dirs))]
    map_dirs: Vec<(String, String)>,

    #[structopt(long, default_value = "16777216")]
    stack_size: usize,

    #[structopt(short)]
    output: PathBuf,

    #[structopt(long)]
    save_temps: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let workspace = Workspace::new(PathBuf::from(".rbwasm").canonicalize()?, opt.save_temps);
    log::info!("install build toolchain...");
    let toolchain = install_build_toolchain(&workspace)?;
    log::info!("build cruby...");
    let ruby_source = BuildSource::GitHub {
        owner: String::from("kateinoigakukun"),
        repo: String::from("ruby"),
        git_ref: String::from("834e12525261d756da85b9b880dabe8407084902"),
    };
    let cruby = build_cruby(&workspace, &toolchain, &ruby_source)?;

    let fs_obj = if !opt.map_dirs.is_empty() {
        log::info!("generating vfs image...");
        let fs_c_src = wasi_vfs_mkfs::generate_c_source(&opt.map_dirs)?;
        let clang = toolchain.wasi_sdk.join("bin/clang");
        let fs_obj = wasi_vfs_mkfs::generate_obj(&fs_c_src, &clang.to_string_lossy())?;
        Some(fs_obj)
    } else {
        None
    };
    {
        let wasm_ld = toolchain.wasi_sdk.join("bin/wasm-ld");
        let mut link = Command::new(wasm_ld);
        link.arg(cruby.install_dir.join("bin/ruby"));
        link.args(["--stack-first", "-z"]);
        link.arg(format!("stack-size={}", opt.stack_size));
        link.arg("-o");
        link.arg(&opt.output);

        fn link_inner(mut link: Command, workspace: &Workspace) -> anyhow::Result<ExitStatus> {
            workspace.with_tempfile("libwasi_vfs.a", |libvfs, libvfs_path| {
                libvfs.write_all(std::include_bytes!(std::concat!(
                    std::env!("OUT_DIR"),
                    "/wasi-vfs-target/wasm32-unknown-unknown/release/libwasi_vfs.a"
                )))?;

                link.arg(libvfs_path);
                log::debug!("link single ruby binary: {:?}", link);
                let status = link
                    .status()
                    .with_context(|| format!("failed to spawn linker"))?;
                Ok(status)
            })?
        }

        let status = if let Some(bytes) = fs_obj {
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
    }
    {
        let mut wasm_opt = Command::new(toolchain.wasm_opt);
        wasm_opt.arg(&opt.output);
        wasm_opt.arg("--asyncify");
        wasm_opt.arg("-O");
        wasm_opt.arg("--pass-arg=asyncify-ignore-imports");
        wasm_opt.arg("-o");
        wasm_opt.arg(&opt.output);
        log::debug!("asyncify ruby binary: {:?}", wasm_opt);
        let status = wasm_opt
            .status()
            .with_context(|| format!("failed to spawn wasm-opt"))?;
        if !status.success() {
            bail!("wasm-opt failed")
        }
    }
    Ok(())
}
