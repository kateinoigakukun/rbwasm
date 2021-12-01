use std::{process::Command, path::Path};

pub fn trace_command_exec(cmd: &Command, cwd: Option<&Path>) {
    if let Some(cwd) = cwd {
        log::info!("Running {:?} in {:?}", cmd, cwd);
    } else {
        log::info!("Running {:?}", cmd);
    }
}
