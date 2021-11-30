use std::path::PathBuf;

use rbwasm::{install_build_toolchain, build_cruby, Workspace, BuildSource};

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let workspace = Workspace::new(PathBuf::from(".rbwasm").canonicalize()?);
    log::info!("install build toolchain...");
    let toolchain = install_build_toolchain(&workspace)?;
    log::info!("build cruby...");
    let ruby_source = BuildSource::GitHub {
        owner: String::from("kateinoigakukun"),
        repo: String::from("ruby"),
        git_ref: String::from("834e12525261d756da85b9b880dabe8407084902"),
    };
    build_cruby(&workspace, &toolchain, &ruby_source)?;
    Ok(())
}
