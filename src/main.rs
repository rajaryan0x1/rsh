use std::env;
use std::fs::{metadata, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct ParsedCommand {
    args: Vec<String>,
    stdout_file: Option<String>,
}

const BUILTINS: [&str; 5] = ["type", "echo", "exit", "pwd", "cd"];

unsafe extern "C" {
    fn dup(fd: i32) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
}

fn parse_command(command: &str) -> ParsedCommand {
    let mut parts = Vec::new();
    let mut current = String::new();

    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;

    for c in command.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }

        match c {
            '\\' if !in_single_quotes => {
                escaped = true;
            }

            '\'' if !in_double_quotes => {
                in_single_quotes = !in_single_quotes;
            }

            '"' if !in_single_quotes => {
                in_double_quotes = !in_double_quotes;
            }

            c if c.is_whitespace() && !in_single_quotes && !in_double_quotes => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }

            _ => {
                current.push(c);
            }
        }
    }

    if escaped {
        current.push('\\');
    }

    if !current.is_empty() {
        parts.push(current);
    }

    let mut args = Vec::new();
    let mut stdout_file = None;

    let mut i = 0;

    while i < parts.len() {
        match parts[i].as_str() {
            ">" | "1>" => {
                if i + 1 < parts.len() {
                    stdout_file = Some(parts[i + 1].clone());
                    i += 2;
                    continue;
                }
            }

            _ => {}
        }

        args.push(parts[i].clone());
        i += 1;
    }

    ParsedCommand { args, stdout_file }
}

fn find_exe(cmd: &str) -> Option<PathBuf> {
    let path = env::var("PATH").unwrap_or_default();

    for dir in path.split(':') {
        let candidate = PathBuf::from(dir).join(cmd);

        if let Ok(metadata) = metadata(&candidate) {
            if metadata.is_file() && (metadata.permissions().mode() & 0o111 != 0) {
                return Some(candidate);
            }
        }
    }

    None
}

fn redirect_stdout<F>(output_file: &str, func: F)
where
    F: FnOnce(),
{
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output_file)
        .expect("failed to open output file");

    let saved_stdout = unsafe { dup(1) };

    if saved_stdout == -1 {
        panic!("failed to duplicate stdout");
    }

    let result = unsafe { dup2(file.as_raw_fd(), 1) };

    if result == -1 {
        panic!("failed to redirect stdout");
    }

    func();

    io::stdout().flush().unwrap();

    let result = unsafe { dup2(saved_stdout, 1) };

    if result == -1 {
        panic!("failed to restore stdout");
    }

    unsafe {
        close(saved_stdout);
    }
}

fn execute_builtin(parts: &ParsedCommand) -> bool {
    match parts.args[0].as_str() {
        "exit" => {
            if parts.args.len() == 1
                || (parts.args.len() == 2 && parts.args[1] == "0")
            {
                std::process::exit(0);
            }

            true
        }

        "echo" => {
            println!("{}", parts.args[1..].join(" "));
            true
        }

        "pwd" => {
            println!("{}", env::current_dir().unwrap().display());
            true
        }

        "cd" => {
            let path = if parts.args.len() < 2 || parts.args[1] == "~" {
                env::var("HOME").unwrap()
            } else {
                parts.args[1].to_string()
            };

            if let Err(_) = env::set_current_dir(&path) {
                println!("cd: {}: No such file or directory", path);
            }

            true
        }

        "type" => {
            if parts.args.len() != 2 {
                return true;
            }

            let cmd = parts.args[1].as_str();

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

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut input = String::new();

        if io::stdin().read_line(&mut input).unwrap() == 0 {
            break;
        }

        let command = input.trim();

        if command.is_empty() {
            continue;
        }

        let parts = parse_command(command);

        if parts.args.is_empty() {
            continue;
        }

        if BUILTINS.contains(&parts.args[0].as_str()) {
            if let Some(output_file) = &parts.stdout_file {
                redirect_stdout(output_file, || {
                    execute_builtin(&parts);
                });
            } else {
                execute_builtin(&parts);
            }

            continue;
        }

        let cmd = parts.args[0].as_str();

        if find_exe(cmd).is_some() {
            let mut command = Command::new(cmd);

            command.args(parts.args.iter().skip(1));

            if let Some(output_file) = parts.stdout_file {
                let file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(output_file)
                    .expect("failed to open output file");

                command.stdout(Stdio::from(file));
            }

            command
                .status()
                .expect("failed to execute the command");
        } else {
            println!("{cmd}: command not found");
        }
    }
}