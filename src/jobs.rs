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
    stopped: bool,
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
    let number = next_number(&jobs);
    println!("[{number}] {}", child.id());
    jobs.push(Job {
        number,
        seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
        command: text.to_string(),
        child,
        finished: false,
        stopped: false,
    });
}

pub(crate) fn adopt(child: Child, text: &str) {
    let mut jobs = JOBS.lock().expect("job table lock");
    let number = next_number(&jobs);
    println!("[{number}]+  Stopped                 {text}");
    jobs.push(Job {
        number,
        seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
        command: text.to_string(),
        child,
        finished: false,
        stopped: true,
    });
}

pub(crate) fn foreground(job_number: Option<u32>) -> i32 {
    let mut jobs = JOBS.lock().expect("job table lock");
    let Some(index) = find_job(&jobs, job_number) else {
        if let Some(number) = job_number {
            println!("fg: %{number}: no such job");
        } else {
            println!("fg: no current job");
        }
        return 1;
    };
    let mut job = jobs.remove(index);
    let command = job.command.clone();
    let pid = job.child.id() as i32;
    let stopped = job.stopped;
    drop(jobs);

    println!("{command}");
    if stopped {
        unsafe { kill(pid, 18) };
    }
    let shell_group = unsafe { getpgrp() };
    unsafe { tcsetpgrp(0, pid) };
    let status = job.child.wait();
    unsafe { tcsetpgrp(0, shell_group) };
    match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 1,
    }
}

pub(crate) fn background(job_number: Option<u32>) -> i32 {
    let mut jobs = JOBS.lock().expect("job table lock");
    let Some(index) = find_job(&jobs, job_number) else {
        if let Some(number) = job_number {
            println!("bg: %{number}: no such job");
        } else {
            println!("bg: no current job");
        }
        return 1;
    };
    let pid = jobs[index].child.id() as i32;
    if jobs[index].stopped {
        unsafe { kill(pid, 18) };
        jobs[index].stopped = false;
    }
    println!("{}", format_job(&jobs[index], &jobs));
    0
}

fn find_job(jobs: &[Job], job_number: Option<u32>) -> Option<usize> {
    match job_number {
        Some(number) => jobs.iter().position(|job| job.number == number),
        None => jobs
            .iter()
            .enumerate()
            .max_by_key(|(_, job)| job.seq)
            .map(|(index, _)| index),
    }
}

pub(crate) fn list() {
    let lines = take_lines(false);
    for line in lines {
        println!("{line}");
    }
}

pub(crate) fn notify() {
    let lines = take_lines(true);
    for line in lines {
        println!("{line}");
    }
}

fn take_lines(finished_only: bool) -> Vec<String> {
    let mut jobs = JOBS.lock().expect("job table lock");
    for job in jobs.iter_mut() {
        refresh(job);
    }
    let mut entries: Vec<_> = jobs.iter().collect();
    entries.sort_by_key(|job| job.number);
    let lines = entries
        .into_iter()
        .filter(|job| !finished_only || job.finished)
        .map(|job| format_job(job, &jobs))
        .collect();
    // A finished job is reported once, then dropped from the table.
    jobs.retain(|job| !job.finished);
    lines
}

fn next_number(jobs: &[Job]) -> u32 {
    let mut number = 1;
    while jobs.iter().any(|job| job.number == number) {
        number += 1;
    }
    number
}

fn refresh(job: &mut Job) {
    if job.finished {
        return;
    }

    let mut status = 0;
    let result = unsafe { waitpid(job.child.id() as i32, &mut status, 1 | 2) };
    if result > 0 {
        if status & 0xff == 0x7f {
            job.stopped = true;
        } else {
            job.finished = true;
            job.stopped = false;
        }
        return;
    }

    if result == -1 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
        job.finished = true;
    }
}

fn format_job(job: &Job, jobs: &[Job]) -> String {
    let status = if job.finished {
        "Done"
    } else if job.stopped {
        "Stopped"
    } else {
        "Running"
    };
    let mut line = format!(
        "[{}]{}  {status}                 {}",
        job.number,
        marker(job, jobs),
        job.command
    );
    if !job.finished && !job.stopped {
        line.push_str(" &");
    }
    line
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
        if job.finished {
            continue;
        }
        let _ = job.child.kill();
        let _ = job.child.wait();
    }
}

unsafe extern "C" {
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
    fn getpgrp() -> i32;
    fn tcsetpgrp(fd: i32, pgrp: i32) -> i32;
}
