use std::fmt;
use std::{path::Path, process::Command};

macro_rules! info {
    ( $ ( $ arg : tt ) * ) => ( $crate::ui::info_fmt ( format_args ! ( $ ( $ arg ) * ) ) )
}

pub(crate) fn trace_command_exec(cmd: &Command, cwd: Option<&Path>) {
    if let Some(cwd) = cwd {
        info!("running {:?} in {:?}", cmd, cwd);
    } else {
        info!("running {:?}", cmd);
    }
}

pub(crate) fn info_fmt(args: fmt::Arguments<'_>) {
    eprintln!("{} {}", ansi_term::Style::new().bold().paint("info:"), args);
}
