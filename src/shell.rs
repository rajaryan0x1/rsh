use rustyline::error::ReadlineError;
use rustyline::{CompletionType, Config, Editor};

use crate::completion::RshHelper;
use crate::execution::execute;
use crate::parser::{parse_pipeline, split_background};

pub(crate) fn run() {
    // List mode: first TAB beeps when there are multiple matches,
    // second TAB prints them all and redraws the prompt.
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();

    let mut rl = Editor::with_config(config).unwrap();

    rl.set_helper(Some(RshHelper));

    loop {
        // Finished jobs are reported after the previous command and before this prompt.
        crate::jobs::notify();
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
                crate::jobs::shutdown();
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

        let (command_text, background) = split_background(command);
        let commands = match parse_pipeline(command_text) {
            Ok(commands) => commands,
            Err(message) => {
                eprintln!("rsh: {message}");
                continue;
            }
        };
        execute(&commands, background, command_text);
    }
}
