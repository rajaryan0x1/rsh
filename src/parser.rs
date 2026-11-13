pub(crate) struct ParsedCommand {
    pub(crate) args: Vec<String>,
    pub(crate) stdout_file: Option<String>,
    pub(crate) stdout_append: bool,
    pub(crate) stderr_file: Option<String>,
    pub(crate) stderr_append: bool,
}

pub(crate) fn split_background(input: &str) -> (&str, bool) {
    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;
    let mut background_at = None;

    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if !in_single_quotes => escaped = true,
            '\'' if !in_double_quotes => in_single_quotes = !in_single_quotes,
            '"' if !in_single_quotes => in_double_quotes = !in_double_quotes,
            '&' if !in_single_quotes && !in_double_quotes => background_at = Some(index),
            _ => {}
        }
    }

    let Some(index) = background_at else {
        return (input, false);
    };

    if input[index + 1..].chars().all(char::is_whitespace) {
        (input[..index].trim_end(), true)
    } else {
        (input, false)
    }
}

pub(crate) fn parse_pipeline(input: &str) -> Result<Vec<ParsedCommand>, &'static str> {
    let segments = split_pipeline(input);

    if segments.len() > 1 && segments.iter().any(|segment| segment.trim().is_empty()) {
        return Err("syntax error near unexpected token '|'");
    }

    Ok(segments.into_iter().map(parse_command).collect())
}

fn split_pipeline(input: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;

    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if !in_single_quotes => escaped = true,
            '\'' if !in_double_quotes => in_single_quotes = !in_single_quotes,
            '"' if !in_single_quotes => in_double_quotes = !in_double_quotes,
            '|' if !in_single_quotes && !in_double_quotes => {
                segments.push(&input[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }

    segments.push(&input[start..]);
    segments
}

pub(crate) fn parse_command(command: &str) -> ParsedCommand {
    let mut parts = Vec::new();
    let mut current = String::new();

    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;
    let mut word_started = false;

    for c in command.chars() {
        if escaped {
            // Inside double quotes, only ", \, $, `, and newline are escaped.
            // A backslash before any other character is kept.
            let quoted_escape = in_double_quotes && matches!(c, '"' | '\\' | '$' | '`' | '\n');
            if in_double_quotes && !quoted_escape {
                current.push('\\');
            }
            if !quoted_escape || c != '\n' {
                current.push(c);
            }
            escaped = false;
            word_started = true;
            continue;
        }

        match c {
            '\\' if !in_single_quotes => {
                escaped = true;
                word_started = true;
            }

            '\'' if !in_double_quotes => {
                in_single_quotes = !in_single_quotes;
                word_started = true;
            }

            '"' if !in_single_quotes => {
                in_double_quotes = !in_double_quotes;
                word_started = true;
            }

            c if c.is_whitespace() && !in_single_quotes && !in_double_quotes => {
                if word_started {
                    parts.push(std::mem::take(&mut current));
                    word_started = false;
                }
            }

            _ => {
                current.push(c);
                word_started = true;
            }
        }
    }

    if escaped {
        current.push('\\');
        word_started = true;
    }

    if word_started {
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
    use super::{parse_command, parse_pipeline, split_background};

    #[test]
    fn quotes_preserve_whitespace_and_join_adjacent_text() {
        let command = parse_command("echo 'hello world' \"double quoted\" pre'fix'\"suffix\"");
        assert_eq!(
            command.args,
            ["echo", "hello world", "double quoted", "prefixsuffix"]
        );
    }

    #[test]
    fn backslashes_follow_the_quoting_context() {
        let command = parse_command(r#"echo one\ two 'three\four' "five\"six" tail\"#);
        assert_eq!(
            command.args,
            ["echo", "one two", "three\\four", "five\"six", "tail\\"]
        );

        let double_quoted = parse_command(
            r#"echo "hello\nworld" "hello'test'\\'example" "hello\"insidequotes"test\""#,
        );
        assert_eq!(
            double_quoted.args,
            [
                "echo",
                r"hello\nworld",
                r"hello'test'\'example",
                "hello\"insidequotestest\""
            ]
        );
    }

    #[test]
    fn empty_quotes_are_arguments() {
        let command = parse_command(r#"echo "a" "" '' "b""c""#);
        assert_eq!(command.args, ["echo", "a", "", "", "bc"]);
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
    fn pipelines_split_on_unquoted_pipes_without_requiring_spaces() {
        let commands = parse_pipeline("echo hello|cat | cat > 'output file'").unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].args, ["echo", "hello"]);
        assert!(commands[0].stdout_file.is_none());
        assert_eq!(commands[1].args, ["cat"]);
        assert!(commands[1].stdout_file.is_none());
        assert_eq!(commands[2].args, ["cat"]);
        assert_eq!(commands[2].stdout_file.as_deref(), Some("output file"));
        assert!(!commands[2].stdout_append);
    }

    #[test]
    fn quoted_and_escaped_pipes_stay_inside_one_command() {
        let quoted = parse_pipeline("echo 'a | b' \"c | d\" | cat").unwrap();
        assert_eq!(quoted.len(), 2);
        assert_eq!(quoted[0].args, ["echo", "a | b", "c | d"]);
        assert_eq!(quoted[1].args, ["cat"]);

        let escaped = parse_pipeline(r"echo left\|right | cat").unwrap();
        assert_eq!(escaped.len(), 2);
        assert_eq!(escaped[0].args, ["echo", "left|right"]);
        assert_eq!(escaped[1].args, ["cat"]);
    }

    #[test]
    fn redirections_bind_to_the_pipeline_stage_they_are_written_on() {
        let commands = parse_pipeline("echo hello > out 2> err | cat >> more 2>> extra").unwrap();

        assert_eq!(commands[0].args, ["echo", "hello"]);
        assert_eq!(commands[0].stdout_file.as_deref(), Some("out"));
        assert!(!commands[0].stdout_append);
        assert_eq!(commands[0].stderr_file.as_deref(), Some("err"));
        assert!(!commands[0].stderr_append);
        assert_eq!(commands[1].args, ["cat"]);
        assert_eq!(commands[1].stdout_file.as_deref(), Some("more"));
        assert!(commands[1].stdout_append);
        assert_eq!(commands[1].stderr_file.as_deref(), Some("extra"));
        assert!(commands[1].stderr_append);
    }

    #[test]
    fn a_missing_pipeline_command_is_rejected() {
        for input in ["| cat", "echo |", "echo | | cat", "|"] {
            assert!(parse_pipeline(input).is_err(), "{input}");
        }
    }

    #[test]
    fn a_trailing_ampersand_marks_the_command_as_background() {
        assert_eq!(split_background("sleep 500 &"), ("sleep 500", true));
        assert_eq!(split_background("sleep 500&"), ("sleep 500", true));
        assert_eq!(
            split_background("echo \"a & b\""),
            ("echo \"a & b\"", false)
        );
        assert_eq!(split_background(r"echo a \&"), (r"echo a \&", false));
        assert_eq!(split_background("echo a & b"), ("echo a & b", false));
    }

    #[test]
    fn whitespace_only_input_has_no_arguments_or_redirections() {
        let command = parse_command(" \t ");
        assert!(command.args.is_empty());
        assert!(command.stdout_file.is_none());
        assert!(command.stderr_file.is_none());
    }
}
