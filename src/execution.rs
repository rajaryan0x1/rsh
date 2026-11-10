use std::ffi::{CString, c_char, c_void};
use std::io::{self, Write};
use std::process::Command;

use crate::builtins::{BUILTINS, execute_builtin};
use crate::executable::find_exe;
use crate::parser::ParsedCommand;
use crate::redirection::{
    apply_child_redirections, configure_external_redirection, with_builtin_redirection,
};

const SIGPIPE: i32 = 13;

pub(crate) fn execute(commands: &[ParsedCommand]) {
    match commands {
        [] => {}
        [command] => execute_command(command),
        commands => execute_pipeline(commands),
    }
}

fn execute_command(parts: &ParsedCommand) {
    let Some(cmd) = parts.args.first() else {
        return;
    };

    if BUILTINS.contains(&cmd.as_str()) {
        with_builtin_redirection(parts, || {
            execute_builtin(&parts.args);
        });
        return;
    }

    if find_exe(cmd).is_some() {
        let mut command = Command::new(cmd);
        command.args(parts.args.iter().skip(1));
        configure_external_redirection(&mut command, parts);
        command.status().expect("failed to execute the command");
    } else {
        println!("{cmd}: command not found");
    }
}

fn execute_pipeline(commands: &[ParsedCommand]) {
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

    for pid in children {
        wait_for(pid);
    }
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

    if BUILTINS.contains(&cmd.as_str()) {
        execute_builtin(&command.args);
        exit_child(0);
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

fn create_pipe() -> (i32, i32) {
    let mut fds = [0; 2];

    if unsafe { pipe(fds.as_mut_ptr()) } == -1 {
        panic!("failed to create pipe");
    }

    (fds[0], fds[1])
}

fn wait_for(pid: i32) {
    loop {
        let mut status = 0;
        let result = unsafe { waitpid(pid, &mut status, 0) };

        if result == pid {
            break;
        }

        if result == -1 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            break;
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
