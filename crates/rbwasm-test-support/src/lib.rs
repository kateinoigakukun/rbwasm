use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct TestWorkspace {
    #[allow(unused)]
    work_dir: PathBuf,
}

#[macro_export]
macro_rules! init_workspace {
    () => {
        $crate::_internal_init_workspace(std::env!("CARGO_TARGET_TMPDIR"))
    };
}

pub fn _internal_init_workspace(tmpdir: &str) -> TestWorkspace {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let root_dir = PathBuf::from(tmpdir);

    let work_dir = root_dir.join(id.to_string());
    if work_dir.exists() {
        std::fs::remove_dir_all(&work_dir).unwrap();
    }
    std::fs::create_dir_all(&work_dir.join(".rbwasm")).unwrap();
    std::env::set_current_dir(&work_dir).unwrap();
    TestWorkspace { work_dir }
}
