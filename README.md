# rsh

A Unix-style shell written from scratch in Rust.

`rsh` is a minimal shell focused on understanding how command-line shells work at the system level. The project is built with Rust on Linux, with an emphasis on command parsing, process execution, Unix file descriptors, pipes, and standard stream redirection.

## Features

* Interactive shell
* Command execution
* Built-in commands

  * `cd`
  * `echo`
  * `exit`
  * `pwd`
  * `type`
* Command parsing

  * Single quotes
  * Double quotes
  * Backslash escaping
  * Whitespace-separated arguments
* Command history
* TAB command completion

  * Built-in command completion
  * Executable completion through `$PATH`
* Executable lookup through `$PATH`
* Process creation and execution
* Standard output redirection

  * `>`
  * `1>`
  * `>>`
  * `1>>`
* Standard error redirection

  * `2>`
  * `2>>`
* Pipelines

  * `|`
  * Multiple stages
  * Per-command file redirection
* File creation and append-mode redirection
* Unix file descriptor manipulation using

  * `dup`
  * `dup2`
  * `close`
* Basic terminal signal/EOF handling

  * `Ctrl-C`
  * `Ctrl-D`

> More shell functionality is being added as development continues.


## Tech Stack

* **Rust**
* **Cargo**
* **Linux (BTW I USE ARCH)**
* **Neovim + LazyVim**

## Running Locally

Clone the repository:

```bash
git clone git@github.com:rajaryan0x1/rsh.git
cd rsh
```

Build the project:

```bash
cargo build
```

Run the shell:

```bash
cargo run
```

For an optimized release build:

```bash
cargo build --release
```

## Redirection

`rsh` currently supports redirecting standard output and standard error to files.

### Standard output

Overwrite a file with `>` or `1>`:

```bash
echo hello > output.txt
echo hello 1> output.txt
```

Append to a file with `>>` or `1>>`:

```bash
echo first >> output.txt
echo second 1>> output.txt
```

The resulting file contains:

```text
first
second
```

### Standard error

Overwrite a file with `2>`:

```bash
ls nonexistent 2> errors.txt
```

Append errors with `2>>`:

```bash
ls nonexistent1 2>> errors.txt
ls nonexistent2 2>> errors.txt
```

The resulting file contains the errors from both commands.

Standard output and standard error are handled independently, so:

```bash
ls nonexistent >> output.txt
```

still displays the error on the terminal while redirecting standard output to `output.txt`.

## Pipes

`|` connects the standard output of one command to the standard input of the next. Spaces around the operator are optional, and a pipeline can have more than two stages:

```bash
echo hello | cat
echo hello|cat
echo hello | cat | cat
```

A quoted or escaped `|` is an ordinary character:

```bash
echo 'hello | cat'
echo hello \| cat
```

File redirections belong to the command they are written on. The first pipeline below writes `hello` to `output.txt`. The second writes `hello` to `output.txt` and leaves `cat` with an empty standard input:

```bash
echo hello | cat > output.txt
echo hello > output.txt | cat
```

Every stage runs in its own process. `cd` and `exit` inside a pipeline do not change the shell that is waiting for that pipeline to finish. An empty stage is a syntax error:

```bash
echo hello | | cat
```

## Project Structure

```text
rsh/
├── src/
│   ├── main.rs          # Module declarations and startup
│   ├── shell.rs         # Interactive loop and command history
│   ├── parser.rs        # Argument parsing, pipelines, and redirection syntax
│   ├── builtins.rs      # Built-in commands
│   ├── completion.rs    # Command and filename completion
│   ├── executable.rs    # Executable lookup through PATH
│   ├── execution.rs     # Command dispatch, pipelines, and process execution
│   ├── redirection.rs   # File opening and stream redirection
│   └── todo.md
├── tests/
│   └── shell.rs         # Shell integration tests
├── Cargo.toml
├── Cargo.lock
└── README.md
```

The interactive loop parses each line and passes its pipeline to the execution module. A single command is dispatched in the current shell. A pipeline forks one child per stage, connects them with `pipe`, and waits until every child exits. Completion and execution share executable lookup, and both execution paths use the redirection module.

Run the parser, completion, and shell integration tests with:

```bash
cargo test
```

## What I'm Learning

This project is part of my exploration of **Rust, Linux, and systems programming**.

Building a shell provides hands-on experience with:

* Process creation and execution
* Unix process management
* Command parsing
* `stdin` / `stdout` / `stderr`
* Unix file descriptors
* `dup`, `dup2`, and `close`
* File I/O and redirection
* Pipes between processes
* `pipe`, `fork`, `exec`, and `waitpid`
* Rust ownership and error handling
* Executable discovery through `$PATH`
* How shells interact with the operating system

## Status

**Work in progress**

The shell is actively being developed, with more functionality planned.

## License

This project is currently intended for learning and experimentation.
