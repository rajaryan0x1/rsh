# rsh

A Unix-style shell written from scratch in Rust.

`rsh` is a minimal shell focused on understanding how command-line shells work at the system level. The project is built with Rust on Linux, with an emphasis on command parsing, process execution, Unix file descriptors, and standard stream redirection.

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
* Executable lookup through `$PATH`
* Process creation and management
* Standard output redirection

  * `>`
  * `1>`
  * `>>`
  * `1>>`
* Standard error redirection

  * `2>`
  * `2>>`
* File creation and append-mode redirection
* Unix file descriptor manipulation using `dup`, `dup2`, and `close`

> More functionality is being added as development continues.

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

## Project Structure

```text
rsh/
├── src/
│   └── ...
├── Cargo.toml
├── Cargo.lock
├── test.txt
└── README.md
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
* Rust ownership and error handling
* Executable discovery through `$PATH`
* How shells interact with the operating system

## Status

**Work in progress**

The shell is actively being developed, with more functionality planned.

## License

This project is currently intended for learning and experimentation.
