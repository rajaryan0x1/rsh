use std::collections::BTreeMap;
use std::sync::Mutex;

static VARIABLES: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

pub(crate) fn get(name: &str) -> Option<String> {
    VARIABLES.lock().unwrap().get(name).cloned()
}

pub(crate) fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(character) if character.is_ascii_alphabetic() || character == '_' => {}
        _ => return false,
    }
    chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn set(name: &str, value: &str) {
    VARIABLES
        .lock()
        .unwrap()
        .insert(name.to_string(), value.to_string());
}

pub(crate) fn execute(args: &[String]) {
    match args {
        [flag, name] if flag == "-p" => match get(name) {
            Some(value) => {
                let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                println!("declare -- {name}=\"{escaped}\"");
            }
            None => println!("declare: {name}: not found"),
        },
        [assignment] => {
            if let Some((name, value)) = assignment.split_once('=') {
                if is_valid_identifier(name) {
                    set(name, value);
                } else {
                    println!("declare: `{assignment}': not a valid identifier");
                }
            }
        }
        _ => {}
    }
}
