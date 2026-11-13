use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::executable::find_exe;
use crate::parser::ParsedCommand;
use crate::redirection::configure_external_redirection;

struct Job {
    number: u32,
    seq: u64,
    command: String,
    child: Child,
}

static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());
static NEXT_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) fn start(command: &ParsedCommand, text: &str) {
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
    jobs.push(Job {
        number,
        seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
        command: text.to_string(),
        child,
    });
}

pub(crate) fn list() {
    let jobs = JOBS.lock().expect("job table lock");
    let current = jobs.iter().map(|job| job.seq).max();
    let mut entries: Vec<_> = jobs.iter().collect();
    entries.sort_by_key(|job| job.number);

    let lines: Vec<String> = entries
        .iter()
        .map(|job| {
            let marker = if Some(job.seq) == current { '+' } else { ' ' };
            format!(
                "[{}]{marker}  Running                 {} &",
                job.number, job.command
            )
        })
        .collect();
    drop(jobs);

    for line in lines {
        println!("{line}");
    }
}

pub(crate) fn shutdown() {
    let mut jobs = JOBS.lock().expect("job table lock");
    for mut job in jobs.drain(..) {
        let _ = job.child.kill();
        let _ = job.child.wait();
    }
}
