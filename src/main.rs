use rbwasm::{
    asyncify_executable, build_cruby, link_executable, toolchain, BuildSource, LinkerInput,
    Workspace,
};
use std::path::PathBuf;
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
    let toolchain = toolchain::install_build_toolchain(&workspace)?;
    log::info!("build cruby...");
    let ruby_source = BuildSource::GitHub {
        owner: String::from("kateinoigakukun"),
        repo: String::from("ruby"),
        git_ref: String::from("834e12525261d756da85b9b880dabe8407084902"),
    };
    let cruby = build_cruby(&workspace, &toolchain, &ruby_source)?;

    let fs_object = if !opt.map_dirs.is_empty() {
        log::info!("generating vfs image...");
        let fs_c_src = wasi_vfs_mkfs::generate_c_source(&opt.map_dirs)?;
        let clang = toolchain.wasi_sdk.join("bin/clang");
        let fs_obj = wasi_vfs_mkfs::generate_obj(&fs_c_src, &clang.to_string_lossy())?;
        Some(fs_obj)
    } else {
        None
    };
    let linker_input = LinkerInput {
        stack_size: opt.stack_size,
        fs_object: fs_object.as_deref(),
    };

    link_executable(&workspace, &toolchain, &cruby, &linker_input, &opt.output)?;
    asyncify_executable(&toolchain, &opt.output, &opt.output)?;
    Ok(())
}
