use std::{path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

use rbwasm::*;

#[test]
fn test_build_cruby() {
    let space = init_workspace();
    let workspace = Workspace::new(PathBuf::from(".rbwasm").canonicalize().unwrap());
    let toolchain = install_build_toolchain(&workspace).expect("failed toolchain install");
    let ruby_source = BuildSource::GitHub {
        owner: String::from("kateinoigakukun"),
        repo: String::from("ruby"),
        git_ref: String::from("834e12525261d756da85b9b880dabe8407084902"),
    };
    build_cruby(&workspace, &toolchain, &ruby_source).expect("failed build cruby");
    drop(space)
}

struct TestWorkspace {
    #[allow(unused)]
    work_dir: PathBuf,
}

fn init_workspace() -> TestWorkspace {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let root_dir = PathBuf::from(std::env!("CARGO_TARGET_TMPDIR"));

    let work_dir = root_dir.join(id.to_string());
    std::fs::create_dir_all(&work_dir.join(".rbwasm")).unwrap();
    std::env::set_current_dir(&work_dir).unwrap();
    TestWorkspace {
        work_dir
    }
}
