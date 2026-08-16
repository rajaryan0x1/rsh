# rsh

A Unix-style shell written from scratch in Rust.

`rsh` is a minimal shell focused on understanding how command-line shells work at the system level. The project is built with Rust on Linux, with an emphasis on process management, command execution, and Unix system concepts.

## Features

* Interactive shell
* Command execution
* Built-in commands
* Command parsing
* Process creation and management
* Standard input/output handling

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
* stdin / stdout / stderr
* System-level programming
* Rust ownership and error handling
* How shells interact with the operating system

## Status

**Work in progress**

The shell is actively being developed, with more functionality planned.

## License

This project is currently intended for learning and experimentation.

