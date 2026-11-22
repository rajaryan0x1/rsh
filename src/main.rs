mod builtins;
mod completion;
mod executable;
mod execution;
mod history;
mod jobs;
mod parser;
mod programmable;
mod redirection;
mod shell;
mod variables;

fn main() {
    shell::run();
}
