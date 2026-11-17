use std::sync::Mutex;

struct History {
    entries: Vec<String>,
}

static HISTORY: Mutex<History> = Mutex::new(History {
    entries: Vec::new(),
});

pub(crate) fn record(command: &str) {
    HISTORY.lock().unwrap().entries.push(command.to_string());
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
