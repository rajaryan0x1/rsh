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

pub(crate) fn list() {
    let history = HISTORY.lock().unwrap();
    for (index, command) in history.entries.iter().enumerate() {
        println!("    {}  {command}", index + 1);
    }
}
