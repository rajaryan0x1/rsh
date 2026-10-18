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
