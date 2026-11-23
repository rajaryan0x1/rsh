use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct Shell {
    dir: PathBuf,
    histfile: Option<PathBuf>,
}

impl Shell {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rsh-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self {
            dir,
            histfile: None,
        }
    }

    fn with_histfile(mut self, path: PathBuf) -> Self {
        self.histfile = Some(path);
        self
    }

    fn run(&self, input: &str) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rsh"));
        command
            .current_dir(&self.dir)
            .env("PATH", format!("{}:/usr/bin:/bin", self.dir.display()))
            .env("HOME", &self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = &self.histfile {
            command.env("HISTFILE", path);
        } else {
            command.env_remove("HISTFILE");
        }
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        output
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.dir.join(name)).unwrap()
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).unwrap();
    }
}

#[test]
fn finished_job_numbers_are_reused() {
    let shell = Shell::new();
    let fifo = shell.dir.join("fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let releaser = shell.dir.join("releaser");
    fs::write(&releaser, "#!/bin/sh\nprintf 'x\\n' > \"$1\"\n").unwrap();
    fs::set_permissions(&releaser, fs::Permissions::from_mode(0o755)).unwrap();

    let output = shell.run(&format!(
        "cat {} &\nreleaser {}\nsleep 0.05\nsleep 20 &\njobs\nexit\n",
        fifo.display(),
        fifo.display()
    ));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let jobs: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with('['))
        .map(str::to_string)
        .collect();
    assert_eq!(jobs.len(), 4, "{stdout}");
    assert!(jobs[0].starts_with("[1] "), "{stdout}");
    assert_eq!(
        jobs[1],
        format!("[1]+  Done                 cat {}", fifo.display())
    );
    assert!(jobs[2].starts_with("[1] "), "{stdout}");
    assert_ne!(jobs[0], jobs[2], "{stdout}");
    assert_eq!(jobs[3], "[1]+  Running                 sleep 20 &");

    let kept = Shell::new();
    let hole = kept.dir.join("hole");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&hole)
            .status()
            .unwrap()
            .success()
    );
    let release_hole = kept.dir.join("releaser");
    fs::write(&release_hole, "#!/bin/sh\nprintf 'x\\n' > \"$1\"\n").unwrap();
    fs::set_permissions(&release_hole, fs::Permissions::from_mode(0o755)).unwrap();
    let output = kept.run(&format!(
        "sleep 30 &\ncat {} &\nreleaser {}\nsleep 0.05\nsleep 15 &\njobs\nexit\n",
        hole.display(),
        hole.display()
    ));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let reported: Vec<_> = stdout
        .lines()
        .filter(|line| line.contains("Done") || line.contains("Running"))
        .map(str::to_string)
        .collect();
    assert_eq!(
        reported,
        [
            format!("[2]+  Done                 cat {}", hole.display()),
            "[1]-  Running                 sleep 30 &".to_string(),
            "[2]+  Running                 sleep 15 &".to_string(),
        ],
        "{stdout}"
    );
}

#[test]
fn a_finished_job_is_reported_before_the_next_prompt() {
    let shell = Shell::new();
    let fifo = shell.dir.join("fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let releaser = shell.dir.join("releaser");
    fs::write(
        &releaser,
        "#!/bin/sh\nprintf 'hello\\n'\nprintf 'x\\n' > \"$1\"\n",
    )
    .unwrap();
    fs::set_permissions(&releaser, fs::Permissions::from_mode(0o755)).unwrap();

    let output = shell.run(&format!(
        "cat {} &\nreleaser {}\njobs\nexit\n",
        fifo.display(),
        fifo.display()
    ));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let hello = stdout.find("hello\n").expect(&stdout);
    let done = stdout.find("Done").expect(&stdout);
    assert!(hello < done, "{stdout}");
    assert_eq!(stdout.matches("Done").count(), 1, "{stdout}");
    assert_eq!(
        stdout.lines().find(|line| line.contains("Done")),
        Some(format!("[1]+  Done                 cat {}", fifo.display()).as_str())
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn jobs_reaps_several_finished_jobs_and_updates_markers() {
    let shell = Shell::new();
    let first = shell.dir.join("first");
    let second = shell.dir.join("second");
    for fifo in [&first, &second] {
        assert!(
            std::process::Command::new("mkfifo")
                .arg(fifo)
                .status()
                .unwrap()
                .success()
        );
    }
    let releaser = shell.dir.join("releaser");
    fs::write(
        &releaser,
        "#!/bin/sh\nprintf 'x\\n' > \"$1\"\nprintf 'x\\n' > \"$2\"\nprintf 'closed\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&releaser, fs::Permissions::from_mode(0o755)).unwrap();

    let output = shell.run(&format!(
        "sleep 30 &\ncat {} &\ncat {} &\nreleaser {} {}\njobs\nexit\n",
        first.display(),
        second.display(),
        first.display(),
        second.display()
    ));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let reported: Vec<_> = stdout
        .lines()
        .filter(|line| line.contains("Done") || line.contains("Running"))
        .map(str::to_string)
        .collect();
    assert_eq!(
        reported,
        [
            format!("[2]-  Done                 cat {}", first.display()),
            format!("[3]+  Done                 cat {}", second.display()),
            "[1]+  Running                 sleep 30 &".to_string(),
        ]
    );
    let closed = stdout.find("closed\n").expect(&stdout);
    let done = stdout.find("Done").expect(&stdout);
    assert!(closed < done, "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn jobs_reports_one_finished_job_and_then_forgets_it() {
    let shell = Shell::new();
    let output = shell.run("true &\nsleep 0.05\njobs\njobs\nexit\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let reported: Vec<_> = stdout
        .lines()
        .filter(|line| line.contains("Done") || line.contains("Running"))
        .collect();
    assert_eq!(reported, ["[1]+  Done                 true"]);
    assert!(output.stderr.is_empty());
}

#[test]
fn jobs_lists_every_running_job_with_current_and_previous_markers() {
    let shell = Shell::new();
    let output = shell.run("sleep 20 &\njobs\nsleep 30 &\njobs\nsleep 40 &\njobs\nexit\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let listed: Vec<_> = stdout
        .lines()
        .filter(|line| line.contains("Running"))
        .collect();
    assert_eq!(
        listed,
        [
            "[1]+  Running                 sleep 20 &",
            "[1]-  Running                 sleep 20 &",
            "[2]+  Running                 sleep 30 &",
            "[1]   Running                 sleep 20 &",
            "[2]-  Running                 sleep 30 &",
            "[3]+  Running                 sleep 40 &",
        ]
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn jobs_lists_one_running_job() {
    let shell = Shell::new();
    let output = shell.run("sleep 30 &\njobs\nexit\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut lines = stdout.lines();
    assert!(lines.next().unwrap().starts_with("[1] "));
    assert_eq!(
        lines.next(),
        Some("[1]+  Running                 sleep 30 &")
    );
    assert_eq!(lines.next(), None);
    assert!(output.stderr.is_empty());
}

#[test]
fn a_background_command_prints_its_own_output() {
    let shell = Shell::new();
    let printer = shell.dir.join("printer");
    fs::write(&printer, "#!/bin/sh\nprintf 'from-background\\n'\n").unwrap();
    fs::set_permissions(&printer, fs::Permissions::from_mode(0o755)).unwrap();

    let output = shell.run("printer &\nsleep 0.1\necho from-foreground\nexit\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("from-background\n"),
        "background output missing from {stdout:?}"
    );
    assert!(
        stdout.contains("from-foreground\n"),
        "foreground output missing from {stdout:?}"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn a_background_command_prints_its_job_number_and_pid() {
    let shell = Shell::new();
    let output = shell.run("sleep 30 &\nexit\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut lines = stdout.lines();
    let started = lines.next().unwrap();
    let pid: i32 = started
        .strip_prefix("[1] ")
        .unwrap_or_else(|| panic!("job line: {started}"))
        .parse()
        .unwrap();
    assert!(pid > 0);
    assert_eq!(lines.next(), None);
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_unquoted_expansions_are_omitted() {
    let shell = Shell::new();
    let script = shell.dir.join("args");
    fs::write(
        &script,
        "#!/bin/sh\nprintf '%s\\n' \"$#\"\nprintf '<%s>\\n' \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let output = shell.run(
        "declare NAME=bar\ndeclare EMPTY=\nargs ${missing_var_1}_suffix ${NAME} ${missing_var_2} $missing_var_3 $EMPTY\nargs \"$EMPTY\"\necho \"a\" \"\" \"b\"\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "2\n<_suffix>\n<bar>\n1\n<>\na  b\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn variables_expand_inside_braces() {
    let shell = Shell::new();
    let output = shell.run(
        "declare NAME=bar\ndeclare OTHER=baz\necho prefix_${NAME}_suffix\necho ${OTHER}_tail\necho '${NAME}'\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "prefix_bar_suffix\nbaz_tail\n${NAME}\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn variables_expand_in_commands() {
    let shell = Shell::new();
    let output = shell.run(
        "declare NAME=bar\ndeclare OTHER=baz\necho $NAME\necho prefix_$OTHER\necho '$NAME'\necho \"hello $NAME\"\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "bar\nprefix_baz\n$NAME\nhello bar\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn declare_rejects_invalid_variable_names() {
    let shell = Shell::new();
    let output = shell.run(
        "declare _ok=yes\ndeclare 2bad=no\ndeclare bad-name=no\ndeclare -p _ok\ndeclare -p 2bad\ndeclare -p bad-name\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "declare: `2bad=no': not a valid identifier\n\
         declare: `bad-name=no': not a valid identifier\n\
         declare -- _ok=\"yes\"\n\
         declare: 2bad: not found\n\
         declare: bad-name: not found\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn declare_stores_and_replaces_shell_variables() {
    let shell = Shell::new();
    let output =
        shell.run("declare FOO=bar\ndeclare -p FOO\ndeclare FOO=baz\ndeclare -p FOO\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "declare -- FOO=\"bar\"\ndeclare -- FOO=\"baz\"\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn declare_reports_a_missing_variable() {
    let shell = Shell::new();
    let output = shell.run("declare -p missing_variable_7\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "declare: missing_variable_7: not found\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn declare_is_a_shell_builtin() {
    let shell = Shell::new();
    let output = shell.run("type declare\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "declare is a shell builtin\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn history_is_a_shell_builtin() {
    let shell = Shell::new();
    let output = shell.run("type history\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "history is a shell builtin\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn history_lists_every_command_including_itself() {
    let shell = Shell::new();
    let output = shell.run("echo one\necho two\nhistory\nexit\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("one\ntwo\n"));
    assert!(stdout.contains("    1  echo one\n    2  echo two\n    3  history\n"));
    assert!(output.stderr.is_empty());
}

#[test]
fn history_appends_new_commands_to_histfile_on_exit() {
    let shell = Shell::new();
    let path = shell.dir.join("hist");
    fs::write(&path, "echo original\n").unwrap();
    let shell = shell.with_histfile(path);
    let output = shell.run("echo one\necho sneaky >> hist\nhistory\nexit\n");
    assert!(output.stderr.is_empty());
    assert_eq!(
        shell.read("hist"),
        "echo original\nsneaky\necho one\necho sneaky >> hist\nhistory\nexit\n"
    );
}

#[test]
fn history_is_written_to_histfile_on_exit() {
    let shell = Shell::new();
    let path = shell.dir.join("hist");
    fs::write(&path, "").unwrap();
    let shell = shell.with_histfile(path);
    let output = shell.run("echo one\necho two\nhistory\nexit\n");
    assert!(output.stderr.is_empty());
    assert_eq!(shell.read("hist"), "echo one\necho two\nhistory\nexit\n");
}

#[test]
fn history_is_loaded_from_histfile_on_startup() {
    let shell = Shell::new();
    let path = shell.dir.join("hist");
    fs::write(&path, "echo loaded\necho also\n").unwrap();
    let shell = shell.with_histfile(path);
    let output = shell.run("history\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "    1  echo loaded\n    2  echo also\n    3  history\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn history_appends_only_new_commands_to_a_file() {
    let shell = Shell::new();
    fs::write(shell.dir.join("hist"), "echo kept\n").unwrap();
    let output =
        shell.run("echo one\necho two\nhistory -a hist\necho three\nhistory -a hist\nexit\n");
    assert_eq!(output.stdout, b"one\ntwo\nthree\n");
    assert!(output.stderr.is_empty());
    assert_eq!(
        shell.read("hist"),
        "echo kept\necho one\necho two\nhistory -a hist\necho three\nhistory -a hist\n"
    );
}

#[test]
fn history_writes_commands_to_a_file() {
    let shell = Shell::new();
    let output = shell.run("echo one\necho two\nhistory -w hist\nexit\n");
    assert_eq!(output.stdout, b"one\ntwo\n");
    assert!(output.stderr.is_empty());
    assert_eq!(shell.read("hist"), "echo one\necho two\nhistory -w hist\n");
}

#[test]
fn history_reads_commands_from_a_file() {
    let shell = Shell::new();
    fs::write(shell.dir.join("hist"), "echo from-file\necho another\n").unwrap();
    let output = shell.run("history -r hist\nhistory\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "    1  history -r hist\n    2  echo from-file\n    3  echo another\n    4  history\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn up_arrow_recalls_the_previous_command() {
    let transcript = pty_transcript(&["echo first\n", "echo second\n", "\u{1b}[A"]);
    assert!(
        transcript.matches("echo second").count() >= 2,
        "{transcript:?}"
    );
}

#[test]
fn down_arrow_moves_forward_through_history() {
    let transcript = pty_transcript(&["echo first\n", "echo second\n", "\u{1b}[A\u{1b}[A\u{1b}[B"]);
    let recalled_first = transcript.rfind("echo first").expect(&transcript);
    let recalled_second = transcript.rfind("echo second").expect(&transcript);
    assert!(
        recalled_second > recalled_first,
        "down arrow should land back on the newer command: {transcript:?}"
    );
}

#[test]
fn a_recalled_command_runs_when_enter_is_pressed() {
    let transcript = pty_transcript(&["echo hello\n", "\u{1b}[A\n"]);
    assert!(transcript.matches("hello").count() >= 4, "{transcript:?}");
}

#[test]
fn history_can_list_only_the_most_recent_entries() {
    let shell = Shell::new();
    let output = shell.run("echo a\necho b\necho c\nhistory 2\nexit\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("    3  echo c\n    4  history 2\n"));
    assert!(!stdout.contains("echo a"));
    assert!(output.stderr.is_empty());
}

#[test]
fn jobs_is_a_shell_builtin_with_no_jobs_to_report() {
    let shell = Shell::new();
    let output = shell.run("type jobs\njobs\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "jobs is a shell builtin\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn complete_is_a_shell_builtin() {
    let shell = Shell::new();
    let output = shell.run("type complete\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "complete is a shell builtin\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn complete_reports_a_missing_specification() {
    let shell = Shell::new();
    let output = shell.run("complete -p git\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "complete: git: no completion specification\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn complete_unregisters_a_specification() {
    let shell = Shell::new();
    let output = shell.run("complete -C /tmp/git.py git\ncomplete -r git\ncomplete -p git\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "complete: git: no completion specification\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn complete_prints_registered_specifications() {
    let shell = Shell::new();
    let output = shell.run(
        "complete  -C  /tmp/git.py  git\ncomplete -p git\ncomplete  -C  /tmp/docker.py  docker\ncomplete -p docker\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "complete -C '/tmp/git.py' git\ncomplete -C '/tmp/docker.py' docker\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn echo_keeps_literal_backslashes_and_empty_quoted_arguments() {
    let shell = Shell::new();
    let output = shell.run(
        "echo \"hello\\nworld\"\necho \"hello'test'\\\\'example\"\necho \"a\" \"\" \"b\"\necho \"hello\\\"insidequotes\"test\\\"\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hello\\nworld\nhello'test'\\'example\na  b\nhello\"insidequotestest\"\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn builtins_and_external_commands_preserve_quoted_arguments() {
    let shell = Shell::new();
    let output = shell.run(
        "\necho 'hello world' \"from rsh\" escaped\\ space\nprintf '[%s]\\n' 'one two' three\ntype echo\ntype missing-rsh-command\nmissing-rsh-command\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hello world from rsh escaped space\n[one two]\n[three]\necho is a shell builtin\nmissing-rsh-command: not found\nmissing-rsh-command: command not found\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn builtin_redirection_overwrites_appends_and_restores_stdout() {
    let shell = Shell::new();
    let output = shell.run(
        "echo old > output\necho first 1> output\necho second >> output\necho third 1>> output\necho visible\necho last > combined 2> errors\nexit\n",
    );
    assert_eq!(shell.read("output"), "first\nsecond\nthird\n");
    assert_eq!(shell.read("combined"), "last\n");
    assert_eq!(shell.read("errors"), "");
    assert_eq!(output.stdout, b"visible\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn external_redirection_keeps_stdout_and_stderr_separate() {
    let shell = Shell::new();
    let output = shell.run(
        "sh -c 'echo out; echo err >&2' > output 2> errors\nsh -c 'echo more; echo warning >&2' >> output 2>> errors\nsh -c 'echo terminal; echo hidden >&2' 2> hidden\nexit\n",
    );
    assert_eq!(shell.read("output"), "out\nmore\n");
    assert_eq!(shell.read("errors"), "err\nwarning\n");
    assert_eq!(shell.read("hidden"), "hidden\n");
    assert_eq!(output.stdout, b"terminal\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn cd_changes_the_shell_directory_and_uses_home() {
    let shell = Shell::new();
    fs::create_dir(shell.dir.join("child")).unwrap();
    let output = shell.run("cd child\npwd\ncd\npwd\ncd missing\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "{}/child\n{}\ncd: missing: No such file or directory\n",
            shell.dir.display(),
            shell.dir.display()
        )
    );
}

#[test]
fn pipelines_connect_stdout_to_stdin_across_multiple_stages() {
    let shell = Shell::new();
    let output = shell.run(
        "echo hello | cat\necho hello | cat | cat\necho hello|cat\necho 'keep | this' | cat\necho escaped\\|pipe | cat\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hello\nhello\nhello\nkeep | this\nescaped|pipe\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn pipeline_redirections_apply_to_the_command_they_are_written_on() {
    let shell = Shell::new();
    let output = shell.run(
        "echo hello | cat > piped\necho visible\necho stored > kept | cat\nsh -c 'echo out; echo err >&2' 2> stage-err | cat\necho from-pipe | sh -c 'cat >&2' 2> piped-err\nexit\n",
    );
    assert_eq!(shell.read("piped"), "hello\n");
    assert_eq!(shell.read("kept"), "stored\n");
    assert_eq!(shell.read("stage-err"), "err\n");
    assert_eq!(shell.read("piped-err"), "from-pipe\n");
    assert_eq!(output.stdout, b"visible\nout\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn pipelines_larger_than_the_pipe_buffer_do_not_deadlock() {
    let shell = Shell::new();
    let line = "hello from the pipe\n";
    let data = line.repeat(4000);
    fs::write(shell.dir.join("input"), &data).unwrap();
    let output = shell.run("cat input | cat | cat > output\nexit\n");
    assert_eq!(shell.read("output"), data);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn pipeline_stages_do_not_change_the_parent_shell() {
    let shell = Shell::new();
    fs::create_dir(shell.dir.join("child")).unwrap();
    let output = shell.run(
        "cd child | pwd\npwd\necho hello | exit\necho still-running\nmissing-pipeline-command | cat\necho after-missing\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "{dir}\n{dir}\nstill-running\nmissing-pipeline-command: command not found\nafter-missing\n",
            dir = shell.dir.display()
        )
    );
}

#[test]
fn an_empty_pipeline_stage_reports_a_syntax_error_and_continues() {
    let shell = Shell::new();
    let output = shell.run("echo hello | | cat\necho after\nexit\n");
    assert_eq!(output.stdout, b"after\n");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("syntax error near unexpected token '|'")
    );
}

fn pty_transcript(steps: &[&str]) -> String {
    let script = r#"
import os, pty, select, signal, sys, time
binpath = sys.argv[1]
os.environ["TERM"] = "xterm"
signal.alarm(8)
pid, fd = pty.fork()
if pid == 0:
    os.execv(binpath, [binpath])
else:
    def read_quiet():
        data = b""
        overall = time.time() + 1.5
        while time.time() < overall:
            ready, _, _ = select.select([fd], [], [], 0.15)
            if not ready:
                if data:
                    return data
                continue
            try:
                chunk = os.read(fd, 4096)
            except OSError:
                return data
            if not chunk:
                return data
            data += chunk
        return data
    out = read_quiet()
    for step in sys.argv[2:]:
        os.write(fd, step.encode())
        out += read_quiet()
    os.kill(pid, signal.SIGTERM)
    os.waitpid(pid, 0)
    sys.stdout.buffer.write(out)
"#;
    let mut command = Command::new("python3");
    command.arg("-c").arg(script).arg(env!("CARGO_BIN_EXE_rsh"));
    for step in steps {
        command.arg(step);
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn globs_expand_unquoted_names_and_leave_quoted_patterns() {
    let shell = Shell::new();
    fs::write(shell.dir.join("a.txt"), "").unwrap();
    fs::write(shell.dir.join("b.txt"), "").unwrap();
    fs::write(shell.dir.join("c.rs"), "").unwrap();
    let output = shell.run("echo *.txt\necho \"*.txt\"\necho *.missing\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "a.txt b.txt\n*.txt\n*.missing\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn command_substitution_inserts_command_output() {
    let shell = Shell::new();
    let output = shell.run(
        "echo $(echo nested)\necho pre$(echo a b)post\necho \"$(echo a b)\"\necho `echo tick`\necho $(true)\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nested\nprea bpost\na b\ntick\n\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn and_and_or_follow_the_previous_status() {
    let shell = Shell::new();
    let output = shell.run(
        "false || echo from-or\ntrue && echo from-and\ntrue || echo skipped-or\nfalse && echo skipped-and\necho left | cat && echo right\nfalse && echo no\nexit\n",
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "from-or\nfrom-and\nleft\nright\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn input_redirection_feeds_a_file_to_stdin() {
    let shell = Shell::new();
    fs::write(shell.dir.join("note"), "hello from the file\n").unwrap();
    let output = shell.run("cat < note\nexit\n");
    assert_eq!(output.stdout, b"hello from the file\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn executable_lookup_checks_permissions_and_searches_path() {
    let shell = Shell::new();
    let executable = shell.dir.join("example");
    fs::write(&executable, "#!/bin/sh\necho found\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(shell.dir.join("not-executable"), "echo unexpected\n").unwrap();
    fs::set_permissions(
        shell.dir.join("not-executable"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let output = shell.run("type example\nexample\nnot-executable\nexit\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "example is {}\nfound\nnot-executable: command not found\n",
            executable.display()
        )
    );
}
