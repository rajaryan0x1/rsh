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

    let command = before.split_whitespace().next()?;
    let completer = {
        let specs = SPECS.lock().expect("completion specifications lock");
        specs.get(command)?.clone()
    };

    Some(single_candidate(&completer))
}

fn single_candidate(completer: &str) -> Vec<String> {
    let Ok(output) = Command::new(completer).stderr(Stdio::null()).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if lines.len() == 1 { lines } else { Vec::new() }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
