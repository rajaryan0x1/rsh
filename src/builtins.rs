use std::env;

use crate::executable::find_exe;

pub(crate) const BUILTINS: [&str; 5] = ["type", "echo", "exit", "pwd", "cd"];

pub(crate) fn execute_builtin(args: &[String]) -> bool {
    match args[0].as_str() {
        "exit" => {
            if args.len() == 1 || (args.len() == 2 && args[1] == "0") {
                std::process::exit(0);
            }

            true
        }

        "echo" => {
            println!("{}", args[1..].join(" "));
            true
        }

        "pwd" => {
            println!("{}", env::current_dir().unwrap().display());
            true
        }

        "cd" => {
            let path = if args.len() < 2 || args[1] == "~" {
                env::var("HOME").unwrap()
            } else {
                args[1].to_string()
            };

            if let Err(_) = env::set_current_dir(&path) {
                println!("cd: {}: No such file or directory", path);
            }

            true
        }

        "type" => {
            if args.len() != 2 {
                return true;
            }

            let cmd = args[1].as_str();

            if BUILTINS.contains(&cmd) {
                println!("{cmd} is a shell builtin");
            } else if let Some(path) = find_exe(cmd) {
                println!("{cmd} is {}", path.display());
            } else {
                println!("{cmd}: not found");
            }

            true
        }

        _ => false,
    }
}
