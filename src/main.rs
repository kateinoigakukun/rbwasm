use anyhow::bail;
use rbwasm::{
    asyncify_executable, build_cruby, build_rb_wasm_support, builtin_map_paths, link_executable,
    mkargs, mkfs, toolchain, BuildSource, CRubyBuildInput, LinkerInput, MkfsInput,
    RbWasmSupportBuildInput, Workspace, DEFAULT_ENABLED_EXTENSIONS,
};
use std::path::PathBuf;
use structopt::StructOpt;

fn parse_map_dirs(s: &str) -> anyhow::Result<(PathBuf, PathBuf)> {
    let parts: Vec<&str> = s.split("::").collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "must contain exactly one double colon ('::')"
        ));
    }
    Ok((parts[0].into(), parts[1].into()))
}

fn parse_build_src(s: &str) -> anyhow::Result<BuildSource> {
    let mut kind_and_rests = s.split(":");
    let kind = if let Some(kind) = kind_and_rests.next() {
        kind
    } else {
        bail!("no build source kind");
    };
    let rest = kind_and_rests.collect::<Vec<_>>().join(":");
    match kind {
        "github" => {
            let owner_and_rests = rest.split("/").collect::<Vec<_>>();
            if owner_and_rests.len() != 2 {
                bail!("invalid github pattern: only one / should appear");
            }
            let owner = owner_and_rests[0];
            let repo_and_ref = owner_and_rests[1].split("@").collect::<Vec<_>>();
            if repo_and_ref.len() != 2 {
                bail!("invalid github pattern: only one @ should appear");
            }
            let repo = repo_and_ref[0];
            let git_ref = repo_and_ref[1];
            return Ok(BuildSource::GitHub {
                owner: String::from(owner),
                repo: String::from(repo),
                git_ref: String::from(git_ref),
            });
        }
        "path" => return Ok(BuildSource::Dir { path: rest.into() }),
        other => {
            bail!("unknown build source kind: {}", &other)
        }
    }
}

#[derive(StructOpt)]
struct Opt {
    #[structopt(long = "mapdir", number_of_values = 1, value_name = "GUEST_DIR::HOST_DIR", parse(try_from_str = parse_map_dirs))]
    map_dirs: Vec<(PathBuf, PathBuf)>,

    #[structopt(long, default_value = "16777216")]
    stack_size: usize,

    #[structopt(long, default_value = "6144")]
    asyncify_stack_size: usize,

    #[structopt(short)]
    output: PathBuf,

    #[structopt(long)]
    save_temps: bool,

    #[structopt(long)]
    no_builtin_files: bool,

    #[structopt(long)]
    enabled_exts: Option<String>,

    #[structopt(short = "g")]
    with_debuginfo: bool,

    #[structopt(long, default_value = "github:kateinoigakukun/ruby@v3_0_2_wasm-alpha1", parse(try_from_str = parse_build_src))]
    cruby_src: BuildSource,

    #[structopt(long, default_value = "github:kateinoigakukun/rb-wasm-support@0.4.0", parse(try_from_str = parse_build_src))]
    rb_wasm_support_src: BuildSource,

    #[structopt(long = "Xcc", number_of_values = 1)]
    extra_cc_args: Vec<String>,

    #[structopt(long = "Xlinker", number_of_values = 1)]
    extra_linker_args: Vec<String>,

    #[structopt(name = "PRESET_ARGS", last = true)]
    preset_args: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let workspace_dir: PathBuf = std::env::var("RBWASM_ROOT")
        .unwrap_or(String::from(".rbwasm"))
        .into();
    if !workspace_dir.exists() {
        log::debug!("workspace dir doesn't exist. create {:?}", workspace_dir);
        std::fs::create_dir_all(&workspace_dir)?;
    }
    let mut workspace = Workspace::create(workspace_dir.canonicalize()?, opt.save_temps)?;
    let toolchain = toolchain::install_build_toolchain(&workspace)?;
    let rb_wasm_support = build_rb_wasm_support(
        &workspace,
        &toolchain,
        &RbWasmSupportBuildInput {
            source: opt.rb_wasm_support_src,
            asyncify_stack_size: opt.asyncify_stack_size,
            extra_cc_args: &opt.extra_cc_args,
        },
    )?;
    let enabled_extentions = if let Some(exts) = &opt.enabled_exts {
        exts.split(",").collect::<Vec<_>>()
    } else {
        DEFAULT_ENABLED_EXTENSIONS.to_vec()
    };
    let cruby = build_cruby(
        &workspace,
        &toolchain,
        &CRubyBuildInput {
            source: opt.cruby_src,
            asyncify_stack_size: opt.asyncify_stack_size,
            extra_cc_args: &opt.extra_cc_args,
            enabled_extentions,
        },
        &rb_wasm_support,
    )?;

    let installed_ruby_root = cruby.install_dir.join(cruby.prefix.strip_prefix("/")?);
    let mut map_paths = if !opt.no_builtin_files {
        builtin_map_paths(&installed_ruby_root)?
    } else {
        vec![]
    };
    map_paths.extend(opt.map_dirs);

    let mut raw_objects = vec![];

    if !map_paths.is_empty() {
        let input = MkfsInput {
            map_paths,
            host_ruby_root: &installed_ruby_root,
            guest_ruby_root: &cruby.prefix.strip_prefix("/embd-root").unwrap(),
        };
        let bytes = mkfs(&workspace, &toolchain, input)?;
        raw_objects.push(("fs.o".to_string(), bytes));
    }

    if !opt.preset_args.is_empty() {
        let bytes = mkargs(&workspace, &toolchain, &opt.preset_args)?;
        raw_objects.push(("preset_args.o".to_string(), bytes));
    }

    let linker_input = LinkerInput {
        stack_size: opt.stack_size,
        raw_objects: raw_objects,
        extra_args: &opt.extra_linker_args,
    };

    link_executable(
        &mut workspace,
        &toolchain,
        &cruby,
        &linker_input,
        &opt.output,
    )?;
    asyncify_executable(&toolchain, opt.with_debuginfo, &opt.output, &opt.output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::parse_build_src;

    #[test]
    fn parse_build_source_github() {
        let src = parse_build_src("github:rust-lang/rust@main").expect("parse failed");
        match src {
            rbwasm::BuildSource::GitHub {
                owner,
                repo,
                git_ref,
            } => {
                assert_eq!(owner, "rust-lang");
                assert_eq!(repo, "rust");
                assert_eq!(git_ref, "main");
            }
            other => {
                panic!("unexpected build source: {:?}", other);
            }
        }
    }

    #[test]
    fn parse_build_source_path() {
        let src = parse_build_src("path:../rust-lang/rust").expect("parse failed");
        match src {
            rbwasm::BuildSource::Dir { path } => {
                assert_eq!(path.to_string_lossy(), "../rust-lang/rust");
            }
            other => {
                panic!("unexpected build source: {:?}", other);
            }
        }
    }
}
