use rustyline::error::ReadlineError;
use rustyline::{CompletionType, Config, Editor};

use crate::completion::RshHelper;
use crate::execution::execute;
use crate::parser::parse_command;

pub(crate) fn run() {
    // List mode: first TAB beeps when there are multiple matches,
    // second TAB prints them all and redraws the prompt.
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();

    let mut rl = Editor::with_config(config).unwrap();

    rl.set_helper(Some(RshHelper));

    loop {
        let input = match rl.readline("$ ") {
            Ok(line) => {
                rl.add_history_entry(line.as_str()).unwrap();
                line
            }

            Err(ReadlineError::Interrupted) => {
                println!();
                continue;
            }

            Err(ReadlineError::Eof) => {
                println!();
                break;
            }

            Err(err) => {
                eprintln!("readline error: {err}");
                break;
            }
        };

        let command = input.trim();

        if command.is_empty() {
            continue;
        }

        let parts = parse_command(command);
        execute(&parts);
    }
}
