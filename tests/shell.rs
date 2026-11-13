use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct Shell {
    dir: PathBuf,
}

impl Shell {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rsh-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self { dir }
    }

    fn run(&self, input: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rsh"))
            .current_dir(&self.dir)
            .env("PATH", format!("{}:/usr/bin:/bin", self.dir.display()))
            .env("HOME", &self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
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
