mod builtins;
mod completion;
mod executable;
mod execution;
mod parser;
mod redirection;
mod shell;

fn main() {
    shell::run();
}
