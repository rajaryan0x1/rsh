use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::Mutex;

static SPECS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

pub(crate) fn execute_complete(args: &[String]) {
    match args {
        [flag, command] if flag == "-p" => print_specification(command),
        [flag, completer, command] if flag == "-C" => register(command, completer),
        _ => {}
    }
}

fn print_specification(command: &str) {
    let specs = SPECS.lock().expect("completion specifications lock");
    match specs.get(command) {
        Some(completer) => println!("complete -C {} {command}", shell_quote(completer)),
        None => println!("complete: {command}: no completion specification"),
    }
}

fn register(command: &str, completer: &str) {
    SPECS
        .lock()
        .expect("completion specifications lock")
        .insert(command.to_string(), completer.to_string());
}

#[cfg(test)]
pub(crate) fn remove_specification(command: &str) {
    SPECS
        .lock()
        .expect("completion specifications lock")
        .remove(command);
}

pub(crate) fn candidates(line: &str, pos: usize) -> Option<Vec<String>> {
    let before = &line[..pos];
    if !before.chars().any(char::is_whitespace) {
        return None;
    }

    let words = completion_words(before);
    let completer = {
        let specs = SPECS.lock().expect("completion specifications lock");
        specs.get(&words.command)?.clone()
    };

    Some(read_candidates(&completer, &words, line, pos))
}

struct CompletionWords {
    command: String,
    current: String,
    previous: String,
}

fn completion_words(before: &str) -> CompletionWords {
    let trailing_whitespace = before.ends_with(char::is_whitespace);
    let tokens: Vec<&str> = before.split_whitespace().collect();
    let command = tokens.first().copied().unwrap_or("").to_string();

    if trailing_whitespace {
        let previous = tokens.last().copied().unwrap_or("").to_string();
        return CompletionWords {
            command,
            current: String::new(),
            previous,
        };
    }

    let current = tokens.last().copied().unwrap_or("").to_string();
    let previous = if tokens.len() >= 2 {
        tokens[tokens.len() - 2].to_string()
    } else {
        command.clone()
    };
    CompletionWords {
        command,
        current,
        previous,
    }
}

fn read_candidates(
    completer: &str,
    words: &CompletionWords,
    line: &str,
    pos: usize,
) -> Vec<String> {
    let Ok(output) = Command::new(completer)
        .arg(&words.command)
        .arg(&words.current)
        .arg(&words.previous)
        .env("COMP_LINE", line)
        .env("COMP_POINT", pos.to_string())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    // Only words that extend the text already typed participate in the common prefix.
    lines.retain(|candidate| candidate.starts_with(&words.current));
    lines.sort();
    lines
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
