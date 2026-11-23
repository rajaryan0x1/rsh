use std::env;

use crate::executable::find_exe;

pub(crate) const BUILTINS: [&str; 11] = [
    "type", "echo", "exit", "pwd", "cd", "complete", "jobs", "history", "declare", "fg", "bg",
];

fn job_number(arg: &str) -> Option<u32> {
    arg.trim_start_matches('%').parse().ok()
}

pub(crate) fn execute_builtin(args: &[String]) -> Option<i32> {
    match args[0].as_str() {
        "exit" => {
            if args.len() == 1 || (args.len() == 2 && args[1] == "0") {
                crate::history::save_on_exit();
                crate::jobs::shutdown();
                std::process::exit(0);
            }

            Some(0)
        }

        "echo" => {
            println!("{}", args[1..].join(" "));
            Some(0)
        }

        "pwd" => {
            println!("{}", env::current_dir().unwrap().display());
            Some(0)
        }

        "cd" => {
            let path = if args.len() < 2 || args[1] == "~" {
                env::var("HOME").unwrap()
            } else {
                args[1].to_string()
            };

            if let Err(_) = env::set_current_dir(&path) {
                println!("cd: {}: No such file or directory", path);
                return Some(1);
            }

            Some(0)
        }

        "complete" => {
            crate::programmable::execute_complete(&args[1..]);
            Some(0)
        }

        "jobs" => {
            crate::jobs::list();
            Some(0)
        }

        "fg" => Some(crate::jobs::foreground(
            args.get(1).and_then(|arg| job_number(arg)),
        )),

        "bg" => Some(crate::jobs::background(
            args.get(1).and_then(|arg| job_number(arg)),
        )),

        "declare" => {
            crate::variables::execute(&args[1..]);
            Some(0)
        }

        "history" => {
            match args.get(1).map(String::as_str) {
                Some("-a") => {
                    if let Some(path) = args.get(2) {
                        crate::history::append_file(path);
                    }
                }
                Some("-w") => {
                    if let Some(path) = args.get(2) {
                        crate::history::write_file(path);
                    }
                }
                Some("-r") => {
                    if let Some(path) = args.get(2) {
                        crate::history::read_file(path);
                    }
                }
                Some(_) => {
                    let limit = args.get(1).and_then(|arg| arg.parse().ok());
                    crate::history::list(limit);
                }
                None => crate::history::list(None),
            }
            Some(0)
        }

        "type" => {
            if args.len() != 2 {
                return Some(0);
            }

            let cmd = args[1].as_str();

            if BUILTINS.contains(&cmd) {
                println!("{cmd} is a shell builtin");
            } else if let Some(path) = find_exe(cmd) {
                println!("{cmd} is {}", path.display());
            } else {
                println!("{cmd}: not found");
                return Some(1);
            }

            Some(0)
        }

        _ => None,
    }
}
