use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::sync::Mutex;

static VARIABLES: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
static EXPORTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

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
    // The shell runs single-threaded, so updating the process environment is sound.
    if EXPORTED.lock().unwrap().contains(name) {
        unsafe { env::set_var(name, value) };
    }
}

fn export_name(name: &str, value: &str) {
    EXPORTED.lock().unwrap().insert(name.to_string());
    set(name, value);
}

pub(crate) fn execute(args: &[String]) -> i32 {
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
                    return 0;
                }
                println!("declare: `{assignment}': not a valid identifier");
                return 1;
            }
        }
        _ => {}
    }
    0
}

pub(crate) fn execute_export(args: &[String]) -> i32 {
    if args.is_empty() {
        let exported = EXPORTED.lock().unwrap().clone();
        for name in exported {
            if let Some(value) = get(&name) {
                let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                println!("declare -x {name}=\"{escaped}\"");
            }
        }
        return 0;
    }

    let mut status = 0;
    for arg in args {
        if let Some((name, value)) = arg.split_once('=') {
            if is_valid_identifier(name) {
                export_name(name, value);
            } else {
                println!("export: `{arg}': not a valid identifier");
                status = 1;
            }
        } else if is_valid_identifier(arg) {
            let value = get(arg).unwrap_or_default();
            export_name(arg, &value);
        } else {
            println!("export: `{arg}': not a valid identifier");
            status = 1;
        }
    }
    status
}
