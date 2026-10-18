use std::process::Command;

use crate::builtins::{BUILTINS, execute_builtin};
use crate::executable::find_exe;
use crate::parser::ParsedCommand;
use crate::redirection::{configure_external_redirection, with_builtin_redirection};

pub(crate) fn execute(parts: &ParsedCommand) {
    let Some(cmd) = parts.args.first() else {
        return;
    };

    if BUILTINS.contains(&cmd.as_str()) {
        with_builtin_redirection(parts, || {
            execute_builtin(&parts.args);
        });
        return;
    }

    if find_exe(cmd).is_some() {
        let mut command = Command::new(cmd);
        command.args(parts.args.iter().skip(1));
        configure_external_redirection(&mut command, parts);
        command.status().expect("failed to execute the command");
    } else {
        println!("{cmd}: command not found");
    }
}
