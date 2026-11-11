use std::env;

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Helper};

use crate::builtins::BUILTINS;
use crate::executable::find_exe;
use crate::programmable;

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
            if let Some(candidates) = programmable::candidates(line, pos) {
                return Ok((start, completion_pairs(candidates)));
            }
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

fn completion_pairs(candidates: Vec<String>) -> Vec<Pair> {
    candidates
        .into_iter()
        .map(|candidate| Pair {
            display: candidate.clone(),
            replacement: format!("{candidate} "),
        })
        .collect()
}

fn complete_filename(prefix: &str) -> Vec<Pair> {
    let (directory, filename_prefix) = match prefix.rfind('/') {
        Some(index) => prefix.split_at(index + 1),
        None => ("", prefix),
    };
    let search_directory = if directory.is_empty() { "." } else { directory };

    let mut matches: Vec<Pair> = std::fs::read_dir(search_directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with(filename_prefix) {
                return None;
            }

            // A finished directory stays open for another completion. A finished
            // file gets a space. The slash and space are not part of an
            // ambiguous prefix, so several matches can still share the text
            // typed so far.
            let is_directory = entry.path().is_dir();
            let suffix = if is_directory { "/" } else { " " };
            let display = if is_directory {
                format!("{name}/")
            } else {
                name.clone()
            };
            Some(Pair {
                display,
                replacement: format!("{directory}{name}{suffix}"),
            })
        })
        .collect();

    matches.sort_by(|a, b| a.display.cmp(&b.display));
    matches
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rustyline::history::DefaultHistory;

    use super::*;

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    struct TestDirectory {
        root: PathBuf,
        nested: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let root = PathBuf::from("target").join(format!(
                "rsh-completion-test-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            let nested = root.join("path/to");
            fs::create_dir_all(&nested).unwrap();
            for name in ["other.txt", "first.txt", "file.txt"] {
                fs::write(nested.join(name), "").unwrap();
            }
            Self { root, nested }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

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

    #[test]
    fn nested_relative_completion_replaces_the_entire_argument() {
        let directory = TestDirectory::new();
        let prefix = format!("{}/file", directory.nested.display());
        let line = format!("cat {prefix} ignored");
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (start, matches) = RshHelper
            .complete(&line, 4 + prefix.len(), &context)
            .unwrap();

        assert_eq!(start, 4);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].display, "file.txt");
        assert_eq!(
            matches[0].replacement,
            format!("{}/file.txt ", directory.nested.display())
        );
    }

    #[test]
    fn absolute_path_completion_preserves_the_directory() {
        let directory = TestDirectory::new();
        let absolute = env::current_dir().unwrap().join(&directory.nested);
        let matches = complete_filename(&format!("{}/file", absolute.display()));

        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement,
            format!("{}/file.txt ", absolute.display())
        );
    }

    #[test]
    fn nested_matches_are_filtered_and_sorted() {
        let directory = TestDirectory::new();
        let matches = complete_filename(&format!("{}/fi", directory.nested.display()));

        let replacements: Vec<_> = matches
            .iter()
            .map(|pair| pair.replacement.as_str())
            .collect();
        assert_eq!(
            replacements,
            [
                format!("{}/file.txt ", directory.nested.display()),
                format!("{}/first.txt ", directory.nested.display()),
            ]
        );
    }

    #[test]
    fn trailing_slash_completes_all_entries_in_the_directory() {
        let directory = TestDirectory::new();
        let matches = complete_filename(&format!("{}/", directory.nested.display()));

        let names: Vec<_> = matches.iter().map(|pair| pair.display.as_str()).collect();
        assert_eq!(names, ["file.txt", "first.txt", "other.txt"]);
    }

    #[test]
    fn missing_directories_and_unmatched_prefixes_have_no_candidates() {
        let directory = TestDirectory::new();

        assert!(complete_filename(&format!("{}/missing/f", directory.nested.display())).is_empty());
        assert!(complete_filename(&format!("{}/unknown", directory.nested.display())).is_empty());
    }

    #[test]
    fn current_directory_completion_does_not_add_a_directory_prefix() {
        let matches = complete_filename("Cargo.t");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacement, "Cargo.toml ");
    }

    #[test]
    fn directory_completion_allows_continued_filename_completion() {
        let directory = TestDirectory::new();
        let prefix = format!("{}/path/t", directory.root.display());
        let line = format!("cat {prefix}");
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (start, matches) = RshHelper.complete(&line, line.len(), &context).unwrap();

        assert_eq!(start, 4);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement,
            format!("{}/", directory.nested.display())
        );

        let line = format!("cat {}file", matches[0].replacement);
        let (start, matches) = RshHelper.complete(&line, line.len(), &context).unwrap();

        assert_eq!(start, 4);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement,
            format!("{}/file.txt ", directory.nested.display())
        );
    }

    #[test]
    fn absolute_directory_completion_appends_a_slash_without_a_space() {
        let directory = TestDirectory::new();
        let absolute = env::current_dir().unwrap().join(&directory.root);
        let matches = complete_filename(&format!("{}/path/t", absolute.display()));

        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement,
            format!("{}/path/to/", absolute.display())
        );
    }

    #[test]
    fn current_directory_completion_appends_a_slash_to_directories() {
        let matches = complete_filename("src");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacement, "src/");
    }

    #[test]
    fn multiple_matches_list_files_and_directories_in_order() {
        let directory = TestDirectory::new();
        fs::create_dir(directory.nested.join("data")).unwrap();
        let matches = complete_filename(&format!("{}/", directory.nested.display()));

        let listed: Vec<_> = matches.iter().map(|pair| pair.display.as_str()).collect();
        assert_eq!(listed, ["data/", "file.txt", "first.txt", "other.txt"]);
        assert_eq!(
            matches[0].replacement,
            format!("{}/data/", directory.nested.display())
        );
        assert!(matches[1].replacement.ends_with("file.txt "));
    }

    #[test]
    fn partial_completion_uses_the_longest_shared_prefix() {
        let directory = TestDirectory::new();
        fs::create_dir(directory.root.join("xyz_apple")).unwrap();
        fs::create_dir(directory.root.join("xyz_apple_banana")).unwrap();
        fs::write(directory.root.join("xyz_apple_banana_cherry.txt"), "").unwrap();

        let matches = complete_filename(&format!("{}/xyz_", directory.root.display()));
        assert_eq!(
            longest_common_prefix(&matches),
            format!("{}/xyz_apple", directory.root.display())
        );

        let matches = complete_filename(&format!("{}/xyz_apple_", directory.root.display()));
        assert_eq!(
            longest_common_prefix(&matches),
            format!("{}/xyz_apple_banana", directory.root.display())
        );

        let matches = complete_filename(&format!("{}/xyz_apple_banana_", directory.root.display()));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement,
            format!("{}/xyz_apple_banana_cherry.txt ", directory.root.display())
        );
    }

    #[test]
    fn later_arguments_complete_independently() {
        let directory = TestDirectory::new();
        let first = format!("{}/other.txt", directory.nested.display());
        let second = format!("{}/file", directory.nested.display());
        let line = format!("cat {first} {second} ignored");
        let cursor = 4 + first.len() + 1 + second.len();
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (start, matches) = RshHelper.complete(&line, cursor, &context).unwrap();

        assert_eq!(start, 4 + first.len() + 1);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].display, "file.txt");
        assert_eq!(
            matches[0].replacement,
            format!("{}/file.txt ", directory.nested.display())
        );
    }

    #[test]
    fn directory_symlinks_can_be_completed_and_traversed() {
        let directory = TestDirectory::new();
        std::os::unix::fs::symlink("path/to", directory.root.join("shortcut")).unwrap();
        let matches = complete_filename(&format!("{}/short", directory.root.display()));

        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement,
            format!("{}/shortcut/", directory.root.display())
        );

        let matches = complete_filename(&format!("{}file", matches[0].replacement));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement,
            format!("{}/shortcut/file.txt ", directory.root.display())
        );
    }

    #[test]
    fn an_empty_completer_does_not_fall_back_to_filenames() {
        let directory = TestDirectory::new();
        fs::write(directory.nested.join("xyz.txt"), "").unwrap();
        let completer = write_executable(&directory.root, "none", "#!/bin/sh\nexit 0\n");
        let _registered = Registered::new("git", &completer);
        let line = format!("git {}/xy", directory.nested.display());
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (_start, matches) = RshHelper.complete(&line, line.len(), &context).unwrap();

        assert!(matches.is_empty());
    }

    #[test]
    fn a_registered_completer_supplies_one_argument_candidate() {
        let directory = TestDirectory::new();
        let completer = write_executable(
            &directory.root,
            "single",
            "#!/bin/sh\nprintf '%s\\n' clone\n",
        );
        let _registered = Registered::new("git", &completer);
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (start, matches) = RshHelper.complete("git ", 4, &context).unwrap();

        assert_eq!(start, 4);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].display, "clone");
        assert_eq!(matches[0].replacement, "clone ");
    }

    struct Registered {
        command: String,
    }

    impl Registered {
        fn new(command: &str, completer: &Path) -> Self {
            crate::programmable::execute_complete(&[
                "-C".to_string(),
                completer.display().to_string(),
                command.to_string(),
            ]);
            Self {
                command: command.to_string(),
            }
        }
    }

    impl Drop for Registered {
        fn drop(&mut self) {
            crate::programmable::remove_specification(&self.command);
        }
    }

    fn write_executable(directory: &Path, name: &str, body: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn longest_common_prefix(matches: &[Pair]) -> String {
        let mut prefix = matches[0].replacement.clone();
        for candidate in &matches[1..] {
            while !candidate.replacement.starts_with(&prefix) {
                prefix.pop();
            }
        }
        prefix
    }
}
