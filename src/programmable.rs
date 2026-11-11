use std::collections::BTreeMap;
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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
