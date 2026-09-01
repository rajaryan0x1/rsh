use std::env;
use std::fs::{metadata, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Editor, Helper};

struct ParsedCommand {
    args: Vec<String>,
    stdout_file: Option<String>,
    stdout_append: bool,
    stderr_file: Option<String>,
    stderr_append: bool,
}

const BUILTINS: [&str; 5] = ["type", "echo", "exit", "pwd", "cd"];

struct RshHelper;

impl Completer for RshHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before_cursor = &line[..pos];

        // Only complete the command itself for now.
        // Example:
        // ech<TAB> -> echo<space>
        if before_cursor.contains(char::is_whitespace) {
            return Ok((pos, Vec::new()));
        }

        let prefix = before_cursor;
        let mut matches = Vec::new();

        // Builtin completion
      
        for builtin in BUILTINS {
            if builtin.starts_with(prefix) {
                matches.push(Pair {
                    display: builtin.to_string(),
                    replacement: format!("{} ", builtin),
                });
            }
        }

       
        // Executable completion
        
        let path = env::var("PATH").unwrap_or_default();

        for dir in path.split(':') {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();

                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name,
                    None => continue,
                };

                if !name.starts_with(prefix) {
                    continue;
                }

                if find_exe(name).is_none() {
                    continue;
                }

                // Avoid duplicate entries
                if matches
                    .iter()
                    .any(|m: &Pair| m.replacement.trim_end() == name)
                {
                    continue;
                }

                matches.push(Pair {
                    display: name.to_string(),
                    replacement: format!("{} ", name),
                });
            }
        }

        Ok((0, matches))
    }
}

impl Hinter for RshHelper {
    type Hint = String;
}

impl Highlighter for RshHelper {}

impl Validator for RshHelper {
    fn validate(
        &self,
        _ctx: &mut ValidationContext<'_>,
    ) -> rustyline::Result<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
}

impl Helper for RshHelper {}

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
    let mut stdout_append = false;
    let mut stderr_file = None;
    let mut stderr_append = false;

    let mut i = 0;

    while i < parts.len() {
        match parts[i].as_str() {
            // stdout overwrite
            ">" | "1>" => {
                if i + 1 < parts.len() {
                    stdout_file = Some(parts[i + 1].clone());
                    stdout_append = false;
                    i += 2;
                    continue;
                }
            }

            // stdout append
            ">>" | "1>>" => {
                if i + 1 < parts.len() {
                    stdout_file = Some(parts[i + 1].clone());
                    stdout_append = true;
                    i += 2;
                    continue;
                }
            }

            // stderr overwrite
            "2>" => {
                if i + 1 < parts.len() {
                    stderr_file = Some(parts[i + 1].clone());
                    stderr_append = false;
                    i += 2;
                    continue;
                }
            }

            // stderr append
            "2>>" => {
                if i + 1 < parts.len() {
                    stderr_file = Some(parts[i + 1].clone());
                    stderr_append = true;
                    i += 2;
                    continue;
                }
            }

            _ => {}
        }

        args.push(parts[i].clone());
        i += 1;
    }

    ParsedCommand {
        args,
        stdout_file,
        stdout_append,
        stderr_file,
        stderr_append,
    }
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

fn redirect_stdout<F>(output_file: &str, append: bool, func: F)
where
    F: FnOnce(),
{
    let file = if append {
        OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(output_file)
            .expect("failed to open output file")
    } else {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(output_file)
            .expect("failed to open output file")
    };

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

fn redirect_stderr<F>(error_file: &str, append: bool, func: F)
where
    F: FnOnce(),
{
    let file = if append {
        OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(error_file)
            .expect("failed to open error file")
    } else {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(error_file)
            .expect("failed to open error file")
    };

    let saved_stderr = unsafe { dup(2) };

    if saved_stderr == -1 {
        panic!("failed to duplicate stderr");
    }

    let result = unsafe { dup2(file.as_raw_fd(), 2) };

    if result == -1 {
        panic!("failed to redirect stderr");
    }

    func();

    let result = unsafe { dup2(saved_stderr, 2) };

    if result == -1 {
        panic!("failed to restore stderr");
    }

    unsafe {
        close(saved_stderr);
    }
}

fn execute_builtin(parts: &ParsedCommand) -> bool {
    match parts.args[0].as_str() {
        "exit" => {
            if parts.args.len() == 1 || (parts.args.len() == 2 && parts.args[1] == "0") {
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
    let mut rl = Editor::new().unwrap();

    rl.set_helper(Some(RshHelper));

    loop {
        let input = match rl.readline("$ ") {
            Ok(line) => {
                rl.add_history_entry(line.as_str()).unwrap();
                line
            }

            Err(ReadlineError::Interrupted) => {
                println!();
                continue;
            }

            Err(ReadlineError::Eof) => {
                println!();
                break;
            }

            Err(err) => {
                eprintln!("readline error: {err}");
                break;
            }
        };

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
                redirect_stdout(output_file, parts.stdout_append, || {
                    if let Some(error_file) = &parts.stderr_file {
                        redirect_stderr(error_file, parts.stderr_append, || {
                            execute_builtin(&parts);
                        });
                    } else {
                        execute_builtin(&parts);
                    }
                });
            } else if let Some(error_file) = &parts.stderr_file {
                redirect_stderr(error_file, parts.stderr_append, || {
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

            // stdout redirection
            if let Some(output_file) = parts.stdout_file {
                let file = if parts.stdout_append {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(true)
                        .open(output_file)
                        .expect("failed to open output file")
                } else {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(output_file)
                        .expect("failed to open output file")
                };

                command.stdout(Stdio::from(file));
            }

            // stderr redirection
            if let Some(error_file) = parts.stderr_file {
                let file = if parts.stderr_append {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(true)
                        .open(error_file)
                        .expect("failed to open error file")
                } else {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(error_file)
                        .expect("failed to open error file")
                };

                command.stderr(Stdio::from(file));
            }

            command
                .status()
                .expect("failed to execute the command");
        } else {
            println!("{cmd}: command not found");
        }
    }
}

// ffi stuff

unsafe extern "C" {
    fn dup(fd: i32) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
}

