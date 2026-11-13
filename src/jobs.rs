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
    finished: bool,
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
        finished: false,
    });
}

pub(crate) fn list() {
    let mut jobs = JOBS.lock().expect("job table lock");
    for job in jobs.iter_mut() {
        refresh(job);
    }
    let lines = status_lines(&jobs);
    // A finished job is reported once, then dropped from the table.
    jobs.retain(|job| !job.finished);
    drop(jobs);

    for line in lines {
        println!("{line}");
    }
}

fn refresh(job: &mut Job) {
    if job.finished {
        return;
    }

    match job.child.try_wait() {
        Ok(Some(_)) | Err(_) => job.finished = true,
        Ok(None) => {}
    }
}

fn status_lines(jobs: &[Job]) -> Vec<String> {
    let mut by_number: Vec<_> = jobs.iter().collect();
    by_number.sort_by_key(|job| job.number);
    by_number
        .into_iter()
        .map(|job| {
            let status = if job.finished { "Done" } else { "Running" };
            let mut line = format!(
                "[{}]{}  {status}                 {}",
                job.number,
                marker(job, jobs),
                job.command
            );
            if !job.finished {
                line.push_str(" &");
            }
            line
        })
        .collect()
}

fn marker(job: &Job, jobs: &[Job]) -> char {
    let mut by_age: Vec<_> = jobs.iter().collect();
    by_age.sort_by_key(|job| job.seq);
    match by_age.iter().position(|candidate| candidate.seq == job.seq) {
        Some(index) if index + 1 == by_age.len() => '+',
        Some(index) if index + 2 == by_age.len() => '-',
        _ => ' ',
    }
}

pub(crate) fn shutdown() {
    let mut jobs = JOBS.lock().expect("job table lock");
    for mut job in jobs.drain(..) {
        let _ = job.child.kill();
        let _ = job.child.wait();
    }
}
