use std::ffi::{CString, c_char, c_void};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::io::FromRawFd;
use std::process::Command;

use crate::builtins::{BUILTINS, execute_builtin};
use crate::executable::find_exe;
use crate::parser::{CommandList, ListOp, ParsedCommand};
use crate::redirection::{
    apply_child_redirections, configure_external_redirection, with_builtin_redirection,
};

const SIGPIPE: i32 = 13;

pub(crate) fn capture(script: &str) -> String {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    let mut fds = [0; 2];
    if unsafe { pipe(fds.as_mut_ptr()) } == -1 {
        return String::new();
    }
    let pid = unsafe { fork() };
    if pid == -1 {
        unsafe {
            close(fds[0]);
            close(fds[1]);
        }
        return String::new();
    }
    if pid == 0 {
        unsafe {
            dup2(fds[1], 1);
            close(fds[0]);
            close(fds[1]);
        }
        let status = match crate::parser::parse_list(script) {
            Ok(list) => run_list(&list, false, script),
            Err(message) => {
                eprintln!("rsh: {message}");
                2
            }
        };
        exit_child(status);
    }
    unsafe { close(fds[1]) };
    let mut output = Vec::new();
    let mut pipe_read = unsafe { File::from_raw_fd(fds[0]) };
    let _ = pipe_read.read_to_end(&mut output);
    drop(pipe_read);
    wait_for(pid);
    String::from_utf8_lossy(&output).into_owned()
}

pub(crate) fn run_list(list: &CommandList, background: bool, command_text: &str) -> i32 {
    if background && list.rest.is_empty() {
        return execute(&list.first, true, command_text);
    }
    let mut status = execute(&list.first, false, command_text);
    for (operator, pipeline) in &list.rest {
        let run = match operator {
            ListOp::And => status == 0,
            ListOp::Or => status != 0,
        };
        if run {
            status = execute(pipeline, false, command_text);
        }
    }
    status
}

pub(crate) fn execute(commands: &[ParsedCommand], background: bool, command_text: &str) -> i32 {
    if background
        && let [command] = commands
        && command
            .args
            .first()
            .is_some_and(|program| !BUILTINS.contains(&program.as_str()))
    {
        crate::jobs::start(command, command_text);
        return 0;
    }

    match commands {
        [] => 0,
        [command] => execute_command(command),
        commands => execute_pipeline(commands),
    }
}

fn execute_command(parts: &ParsedCommand) -> i32 {
    let Some(cmd) = parts.args.first() else {
        return 0;
    };

    if missing_stdin(parts) {
        return 1;
    }

    if BUILTINS.contains(&cmd.as_str()) {
        let mut status = 0;
        with_builtin_redirection(parts, || {
            status = execute_builtin(&parts.args).unwrap_or(0);
        });
        return status;
    }

    if find_exe(cmd).is_some() {
        let mut command = Command::new(cmd);
        command.args(parts.args.iter().skip(1));
        configure_external_redirection(&mut command, parts);
        match command.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(_) => {
                println!("{cmd}: command not found");
                127
            }
        }
    } else {
        println!("{cmd}: command not found");
        127
    }
}

fn execute_pipeline(commands: &[ParsedCommand]) -> i32 {
    // Flush before forking so buffered output is not copied into every child.
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();

    let mut previous_read = None;
    let mut children = Vec::with_capacity(commands.len());

    for (index, command) in commands.iter().enumerate() {
        let is_last = index + 1 == commands.len();
        let output_pipe = if is_last { None } else { Some(create_pipe()) };
        let pid = unsafe { fork() };

        if pid == -1 {
            panic!("failed to fork");
        }

        if pid == 0 {
            run_pipeline_child(command, previous_read, output_pipe);
        }

        // The parent must drop every write end it does not use. Otherwise the
        // next command never sees end-of-file and the pipeline waits forever.
        if let Some(fd) = previous_read.take() {
            unsafe { close(fd) };
        }
        if let Some((read_fd, write_fd)) = output_pipe {
            unsafe { close(write_fd) };
            previous_read = Some(read_fd);
        }
        children.push(pid);
    }

    let mut status = 0;
    for pid in children {
        status = wait_for(pid);
    }
    status
}

fn run_pipeline_child(
    command: &ParsedCommand,
    previous_read: Option<i32>,
    output_pipe: Option<(i32, i32)>,
) -> ! {
    // The Rust runtime ignores SIGPIPE. Restore the default so a command stops
    // writing once the next command closes the pipe.
    unsafe { signal(SIGPIPE, std::ptr::null()) };

    if let Some(fd) = previous_read {
        if unsafe { dup2(fd, 0) } == -1 {
            eprintln!("rsh: failed to redirect stdin");
            exit_child(1);
        }
        unsafe { close(fd) };
    }

    if let Some((read_fd, write_fd)) = output_pipe {
        if unsafe { dup2(write_fd, 1) } == -1 {
            eprintln!("rsh: failed to redirect stdout");
            exit_child(1);
        }
        unsafe {
            close(read_fd);
            close(write_fd);
        }
    }

    // File redirections replace the pipe ends set up above.
    apply_child_redirections(command);
    run_command(command);
}

fn run_command(command: &ParsedCommand) -> ! {
    let Some(cmd) = command.args.first() else {
        exit_child(0);
    };

    if missing_stdin(command) {
        exit_child(1);
    }

    if BUILTINS.contains(&cmd.as_str()) {
        let status = execute_builtin(&command.args).unwrap_or(0);
        exit_child(status);
    }

    if find_exe(cmd).is_none() {
        println!("{cmd}: command not found");
        exit_child(127);
    }

    let c_strings: Vec<CString> = command
        .args
        .iter()
        .map(|arg| CString::new(arg.as_str()).expect("command argument contains a nul byte"))
        .collect();
    let mut argv: Vec<*const c_char> = c_strings.iter().map(|arg| arg.as_ptr()).collect();
    argv.push(std::ptr::null());

    unsafe { execvp(argv[0], argv.as_ptr()) };
    println!("{cmd}: command not found");
    exit_child(127);
}

fn missing_stdin(parts: &ParsedCommand) -> bool {
    if let Some(path) = &parts.stdin_file
        && File::open(path).is_err()
    {
        println!("rsh: {path}: No such file or directory");
        return true;
    }
    false
}

fn create_pipe() -> (i32, i32) {
    let mut fds = [0; 2];

    if unsafe { pipe(fds.as_mut_ptr()) } == -1 {
        panic!("failed to create pipe");
    }

    (fds[0], fds[1])
}

fn wait_for(pid: i32) -> i32 {
    loop {
        let mut status = 0;
        let result = unsafe { waitpid(pid, &mut status, 0) };

        if result == pid {
            return if status & 0x7f == 0 {
                (status >> 8) & 0xff
            } else {
                128 + (status & 0x7f)
            };
        }

        if result == -1 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return 1;
        }
    }
}

fn exit_child(status: i32) -> ! {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    unsafe { _exit(status) }
}

unsafe extern "C" {
    fn fork() -> i32;
    fn pipe(pipefd: *mut i32) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn execvp(file: *const c_char, argv: *const *const c_char) -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn signal(signum: i32, handler: *const c_void) -> *const c_void;
    fn _exit(status: i32) -> !;
}
