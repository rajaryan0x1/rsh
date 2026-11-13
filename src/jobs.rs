use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use crate::executable::find_exe;
use crate::parser::ParsedCommand;
use crate::redirection::configure_external_redirection;

struct Job {
    number: u32,
    child: Child,
}

static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());

pub(crate) fn start(command: &ParsedCommand) {
    let Some(program) = command.args.first() else {
        return;
    };

    if find_exe(program).is_none() {
        println!("{program}: command not found");
        return;
    }

    // Flush before the child inherits the shell's output buffers.
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();

    let mut process = Command::new(program);
    process.args(command.args.iter().skip(1));
    process.process_group(0);
    // The job keeps the shell's streams, so its output appears immediately.
    process
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    configure_external_redirection(&mut process, command);

    let child = match process.spawn() {
        Ok(child) => child,
        Err(_) => {
            println!("{program}: command not found");
            return;
        }
    };

    let mut jobs = JOBS.lock().expect("job table lock");
    let number = jobs.iter().map(|job| job.number).max().unwrap_or(0) + 1;
    println!("[{number}] {}", child.id());
    jobs.push(Job { number, child });
}

pub(crate) fn shutdown() {
    let mut jobs = JOBS.lock().expect("job table lock");
    for mut job in jobs.drain(..) {
        let _ = job.child.kill();
        let _ = job.child.wait();
    }
}
