pub(crate) struct ParsedCommand {
    pub(crate) args: Vec<String>,
    pub(crate) stdout_file: Option<String>,
    pub(crate) stdout_append: bool,
    pub(crate) stderr_file: Option<String>,
    pub(crate) stderr_append: bool,
}

pub(crate) fn parse_command(command: &str) -> ParsedCommand {
    let mut parts = Vec::new();
    let mut current = String::new();

    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;

    for c in command.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }

        match c {
            '\\' if !in_single_quotes => {
                escaped = true;
            }

            '\'' if !in_double_quotes => {
                in_single_quotes = !in_single_quotes;
            }

            '"' if !in_single_quotes => {
                in_double_quotes = !in_double_quotes;
            }

            c if c.is_whitespace() && !in_single_quotes && !in_double_quotes => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }

            _ => {
                current.push(c);
            }
        }
    }

    if escaped {
        current.push('\\');
    }

    if !current.is_empty() {
        parts.push(current);
    }

    let mut args = Vec::new();
    let mut stdout_file = None;
    let mut stdout_append = false;
    let mut stderr_file = None;
    let mut stderr_append = false;

    let mut i = 0;

    while i < parts.len() {
        match parts[i].as_str() {
            // stdout overwrite
            ">" | "1>" => {
                if i + 1 < parts.len() {
                    stdout_file = Some(parts[i + 1].clone());
                    stdout_append = false;
                    i += 2;
                    continue;
                }
            }

            // stdout append
            ">>" | "1>>" => {
                if i + 1 < parts.len() {
                    stdout_file = Some(parts[i + 1].clone());
                    stdout_append = true;
                    i += 2;
                    continue;
                }
            }

            // stderr overwrite
            "2>" => {
                if i + 1 < parts.len() {
                    stderr_file = Some(parts[i + 1].clone());
                    stderr_append = false;
                    i += 2;
                    continue;
                }
            }

            // stderr append
            "2>>" => {
                if i + 1 < parts.len() {
                    stderr_file = Some(parts[i + 1].clone());
                    stderr_append = true;
                    i += 2;
                    continue;
                }
            }

            _ => {}
        }

        args.push(parts[i].clone());
        i += 1;
    }

    ParsedCommand {
        args,
        stdout_file,
        stdout_append,
        stderr_file,
        stderr_append,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_command;

    #[test]
    fn quotes_preserve_whitespace_and_join_adjacent_text() {
        let command = parse_command("echo 'hello world' \"double quoted\" pre'fix'\"suffix\"");
        assert_eq!(
            command.args,
            ["echo", "hello world", "double quoted", "prefixsuffix"]
        );
    }

    #[test]
    fn backslashes_escape_characters_outside_single_quotes() {
        let command = parse_command(r#"echo one\ two 'three\four' "five\"six" tail\"#);
        assert_eq!(
            command.args,
            ["echo", "one two", "three\\four", "five\"six", "tail\\"]
        );
    }

    #[test]
    fn redirection_operators_set_the_stream_and_append_mode() {
        for (operator, stderr, append) in [
            (">", false, false),
            ("1>", false, false),
            (">>", false, true),
            ("1>>", false, true),
            ("2>", true, false),
            ("2>>", true, true),
        ] {
            let command = parse_command(&format!("echo hello {operator} 'output file'"));
            assert_eq!(command.args, ["echo", "hello"]);
            if stderr {
                assert_eq!(command.stderr_file.as_deref(), Some("output file"));
                assert_eq!(command.stderr_append, append);
                assert!(command.stdout_file.is_none());
            } else {
                assert_eq!(command.stdout_file.as_deref(), Some("output file"));
                assert_eq!(command.stdout_append, append);
                assert!(command.stderr_file.is_none());
            }
        }
    }

    #[test]
    fn last_redirection_for_each_stream_wins() {
        let command = parse_command("echo hello >> old > new 2> errors 2>> more-errors");
        assert_eq!(command.args, ["echo", "hello"]);
        assert_eq!(command.stdout_file.as_deref(), Some("new"));
        assert!(!command.stdout_append);
        assert_eq!(command.stderr_file.as_deref(), Some("more-errors"));
        assert!(command.stderr_append);
    }

    #[test]
    fn whitespace_only_input_has_no_arguments_or_redirections() {
        let command = parse_command(" \t ");
        assert!(command.args.is_empty());
        assert!(command.stdout_file.is_none());
        assert!(command.stderr_file.is_none());
    }
}
