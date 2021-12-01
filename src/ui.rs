use std::fmt;
use std::{path::Path, process::Command};

#[macro_export]
macro_rules! ui_info {
    ( $ ( $ arg : tt ) * ) => ( $crate::ui::info_fmt ( format_args ! ( $ ( $ arg ) * ) ) )
}

pub(crate) fn trace_command_exec(cmd: &Command, description: &str, cwd: Option<&Path>) {
    let is_verbose = std::env::var("RBWASM_DEBUG").is_ok();
    if let Some(cwd) = cwd {
        if is_verbose {
            ui_info!("running {} in {:?}: {:?}", description, cmd, cwd);
        } else {
            ui_info!("running {}", description);
        }
    } else {
        if is_verbose {
            ui_info!("running {}: {:?}", description, cmd);
        } else {
            ui_info!("running {}", description);
        }
    }
}

pub(crate) fn info_fmt(args: fmt::Arguments<'_>) {
    eprintln!("{} {}", ansi_term::Style::new().bold().paint("info:"), args);
}
