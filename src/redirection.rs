use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::process::{Command, Stdio};

use crate::parser::ParsedCommand;

pub(crate) fn with_builtin_redirection<F>(parts: &ParsedCommand, func: F)
where
    F: FnOnce(),
{
    if let Some(output_file) = &parts.stdout_file {
        redirect_stdout(output_file, parts.stdout_append, || {
            if let Some(error_file) = &parts.stderr_file {
                redirect_stderr(error_file, parts.stderr_append, func);
            } else {
                func();
            }
        });
    } else if let Some(error_file) = &parts.stderr_file {
        redirect_stderr(error_file, parts.stderr_append, func);
    } else {
        func();
    }
}

pub(crate) fn configure_external_redirection(command: &mut Command, parts: &ParsedCommand) {
    if let Some(output_file) = &parts.stdout_file {
        let file = open_file(output_file, parts.stdout_append).expect("failed to open output file");
        command.stdout(Stdio::from(file));
    }

    if let Some(error_file) = &parts.stderr_file {
        let file = open_file(error_file, parts.stderr_append).expect("failed to open error file");
        command.stderr(Stdio::from(file));
    }
}

fn open_file(path: &str, append: bool) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append)
        .open(path)
}

fn redirect_stdout<F>(output_file: &str, append: bool, func: F)
where
    F: FnOnce(),
{
    let file = open_file(output_file, append).expect("failed to open output file");

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
    let file = open_file(error_file, append).expect("failed to open error file");

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

unsafe extern "C" {
    fn dup(fd: i32) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
}
