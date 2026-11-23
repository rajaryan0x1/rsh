use rustyline::error::ReadlineError;
use rustyline::{Cmd, CompletionType, Config, Editor, KeyCode, KeyEvent, Modifiers};

use crate::completion::RshHelper;
use crate::execution::run_list;
use crate::parser::{parse_list, split_background};

pub(crate) fn run() {
    // List mode: first TAB beeps when there are multiple matches,
    // second TAB prints them all and redraws the prompt.
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();

    let mut rl = Editor::with_config(config).unwrap();

    rl.set_helper(Some(RshHelper));
    rl.bind_sequence(KeyEvent(KeyCode::Up, Modifiers::NONE), Cmd::PreviousHistory);
    rl.bind_sequence(KeyEvent(KeyCode::Down, Modifiers::NONE), Cmd::NextHistory);
    rl.bind_sequence(KeyEvent(KeyCode::Enter, Modifiers::NONE), Cmd::AcceptLine);
    for entry in crate::history::load_startup() {
        rl.add_history_entry(entry).unwrap();
    }

    loop {
        // Finished jobs are reported after the previous command and before this prompt.
        crate::jobs::notify();
        let input = match rl.readline("$ ") {
            Ok(line) => line,

            Err(ReadlineError::Interrupted) => {
                println!();
                continue;
            }

            Err(ReadlineError::Eof) => {
                println!();
                crate::history::save_on_exit();
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

        crate::history::record(command);
        rl.add_history_entry(command).unwrap();

        let (command_text, background) = split_background(command);
        let list = match parse_list(command_text) {
            Ok(list) => list,
            Err(message) => {
                eprintln!("rsh: {message}");
                continue;
            }
        };
        run_list(&list, background, command_text);
        for entry in crate::history::take_for_readline() {
            rl.add_history_entry(entry).unwrap();
        }
    }
}
