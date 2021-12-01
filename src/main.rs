use anyhow::bail;
use rbwasm::{
    asyncify_executable, build_cruby, link_executable, mkfs, toolchain, BuildSource, LinkerInput,
    MkfsInput, Workspace,
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

    #[structopt(short)]
    output: PathBuf,

    #[structopt(long)]
    save_temps: bool,

    #[structopt(long, default_value = "github:kateinoigakukun/ruby@834e125", parse(try_from_str = parse_build_src))]
    cruby_src: BuildSource,
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
    let workspace = Workspace::create(workspace_dir.canonicalize()?, opt.save_temps)?;
    let toolchain = toolchain::install_build_toolchain(&workspace)?;
    let cruby = build_cruby(&workspace, &toolchain, &opt.cruby_src)?;

    let fs_object = if !opt.map_dirs.is_empty() {
        let input = MkfsInput {
            map_dirs: opt.map_dirs,
            ruby_root: &cruby.install_dir,
        };
        Some(mkfs(&workspace, &toolchain, input)?)
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

#[cfg(test)]
mod tests {
    use crate::parse_build_src;

    #[test]
    fn parse_build_source_github() {
        let src = parse_build_src("github:rust-lang/rust@main").expect("parse failed");
        match src {
            rbwasm::BuildSource::GitHub { owner, repo, git_ref } => {
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
