use std::env;

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Helper};

use crate::builtins::BUILTINS;
use crate::executable::find_exe;

pub(crate) struct RshHelper;

impl Completer for RshHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before_cursor = &line[..pos];

        if let Some(idx) = before_cursor.rfind(char::is_whitespace) {
            let start = idx + 1;
            let prefix = &before_cursor[start..];
            return Ok((start, complete_filename(prefix)));
        }

        let prefix = before_cursor;
        let mut matches = Vec::new();

        // Builtin completion
        for builtin in BUILTINS {
            if builtin.starts_with(prefix) {
                matches.push(Pair {
                    display: builtin.to_string(),
                    replacement: format!("{} ", builtin),
                });
            }
        }

        // Executable completion
        let path = env::var("PATH").unwrap_or_default();

        for dir in path.split(':') {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();

                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name,
                    None => continue,
                };

                if !name.starts_with(prefix) {
                    continue;
                }

                if find_exe(name).is_none() {
                    continue;
                }

                // Avoid duplicate entries
                if matches
                    .iter()
                    .any(|m: &Pair| m.replacement.trim_end() == name)
                {
                    continue;
                }

                matches.push(Pair {
                    display: name.to_string(),
                    replacement: format!("{} ", name),
                });
            }
        }

        // Alphabetical order for the listing
        matches.sort_by(|a, b| a.display.cmp(&b.display));

        Ok((0, matches))
    }
}

impl Hinter for RshHelper {
    type Hint = String;
}

impl Highlighter for RshHelper {}

impl Validator for RshHelper {
    fn validate(&self, _ctx: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
}

impl Helper for RshHelper {}

fn complete_filename(prefix: &str) -> Vec<Pair> {
    let mut matches: Vec<Pair> = std::fs::read_dir(".")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(prefix))
        .map(|name| Pair {
            display: name.clone(),
            replacement: format!("{} ", name),
        })
        .collect();

    matches.sort_by(|a, b| a.display.cmp(&b.display));
    matches
}

#[cfg(test)]
mod tests {
    use rustyline::history::DefaultHistory;

    use super::*;

    #[test]
    fn command_completion_uses_the_prefix_before_the_cursor() {
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (start, matches) = RshHelper.complete("ech ignored", 3, &context).unwrap();

        assert_eq!(start, 0);
        assert!(
            matches
                .iter()
                .any(|candidate| candidate.display == "echo" && candidate.replacement == "echo ")
        );
        assert!(
            matches
                .iter()
                .all(|candidate| candidate.display.starts_with("ech"))
        );
        assert!(
            matches
                .windows(2)
                .all(|pair| pair[0].display < pair[1].display)
        );
    }
}
