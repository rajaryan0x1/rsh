use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

struct History {
    entries: Vec<String>,
    /// How many entries have already been handed to the line editor.
    synced: usize,
    /// How many entries have already been written to a history file.
    written: usize,
}

static HISTORY: Mutex<History> = Mutex::new(History {
    entries: Vec::new(),
    synced: 0,
    written: 0,
});

pub(crate) fn record(command: &str) {
    let mut history = HISTORY.lock().unwrap();
    history.entries.push(command.to_string());
    history.synced = history.entries.len();
}

pub(crate) fn take_for_readline() -> Vec<String> {
    let mut history = HISTORY.lock().unwrap();
    let lines = history.entries[history.synced..].to_vec();
    history.synced = history.entries.len();
    lines
}

pub(crate) fn write_file(path: &str) {
    let history = HISTORY.lock().unwrap();
    let mut body = history.entries.join("\n");
    if !history.entries.is_empty() {
        body.push('\n');
    }
    let count = history.entries.len();
    drop(history);
    if fs::write(path, body).is_err() {
        println!("history: {path}: cannot write");
        return;
    }
    HISTORY.lock().unwrap().written = count;
}

pub(crate) fn append_file(path: &str) {
    let history = HISTORY.lock().unwrap();
    if history.written >= history.entries.len() {
        return;
    }
    let mut body = history.entries[history.written..].join("\n");
    body.push('\n');
    let count = history.entries.len();
    drop(history);
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(body.as_bytes()));
    if result.is_err() {
        println!("history: {path}: cannot write");
        return;
    }
    HISTORY.lock().unwrap().written = count;
}

pub(crate) fn read_file(path: &str) {
    let Ok(text) = fs::read_to_string(path) else {
        println!("history: {path}: cannot read");
        return;
    };
    let mut history = HISTORY.lock().unwrap();
    for line in text.lines() {
        if !line.is_empty() {
            history.entries.push(line.to_string());
        }
    }
}

pub(crate) fn list(limit: Option<usize>) {
    let history = HISTORY.lock().unwrap();
    let start = match limit {
        Some(count) => history.entries.len().saturating_sub(count),
        None => 0,
    };
    for (index, command) in history.entries.iter().enumerate().skip(start) {
        println!("    {}  {command}", index + 1);
    }
}
