pub(crate) struct ParsedCommand {
    pub(crate) args: Vec<String>,
    pub(crate) stdin_file: Option<String>,
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

#[derive(Clone, Copy)]
pub(crate) enum ListOp {
    And,
    Or,
}

pub(crate) struct CommandList {
    pub(crate) first: Vec<ParsedCommand>,
    pub(crate) rest: Vec<(ListOp, Vec<ParsedCommand>)>,
}

pub(crate) fn parse_list(input: &str) -> Result<CommandList, &'static str> {
    let mut segments = split_list(input)?.into_iter();
    let (_, first_text) = segments.next().unwrap();
    let first = parse_pipeline(first_text)?;
    let mut rest = Vec::new();
    for (operator, text) in segments {
        let Some(operator) = operator else {
            continue;
        };
        rest.push((operator, parse_pipeline(text)?));
    }
    Ok(CommandList { first, rest })
}

fn split_list(input: &str) -> Result<Vec<(Option<ListOp>, &str)>, &'static str> {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut pending = None;
    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;
    let mut index = 0;

    while index < chars.len() {
        let (byte, character) = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }

        let operator = match character {
            '\\' if !in_single_quotes => {
                escaped = true;
                None
            }
            '\'' if !in_double_quotes => {
                in_single_quotes = !in_single_quotes;
                None
            }
            '"' if !in_single_quotes => {
                in_double_quotes = !in_double_quotes;
                None
            }
            '&' | '|' if !in_single_quotes && !in_double_quotes => {
                let next = chars.get(index + 1).map(|(_, character)| *character);
                if character == '&' && next == Some('&') {
                    Some(ListOp::And)
                } else if character == '|' && next == Some('|') {
                    Some(ListOp::Or)
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(operator) = operator {
            let segment = &input[start..byte];
            if segment.trim().is_empty() {
                return Err(operator_error(operator));
            }
            segments.push((pending, segment));
            pending = Some(operator);
            start = chars[index + 1].0 + chars[index + 1].1.len_utf8();
            index += 2;
            continue;
        }

        index += 1;
    }

    let segment = &input[start..];
    if segment.trim().is_empty()
        && let Some(operator) = pending
    {
        return Err(operator_error(operator));
    }
    segments.push((pending, segment));
    Ok(segments)
}

fn operator_error(operator: ListOp) -> &'static str {
    match operator {
        ListOp::And => "syntax error near unexpected token '&&'",
        ListOp::Or => "syntax error near unexpected token '||'",
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

fn contains_glob(text: &str) -> bool {
    text.chars()
        .any(|character| matches!(character, '*' | '?' | '['))
}

fn append_parameter(
    name: &str,
    current: &mut String,
    word_started: &mut bool,
    glob: &mut bool,
    quoted: bool,
) {
    if let Some(value) = crate::variables::get(name).filter(|value| !value.is_empty()) {
        if !quoted && contains_glob(&value) {
            *glob = true;
        }
        current.push_str(&value);
        *word_started = true;
    }
}

fn expand_dollar(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    current: &mut String,
    word_started: &mut bool,
    glob: &mut bool,
    quoted: bool,
) {
    if chars.peek() == Some(&'{') {
        chars.next();
        let mut name = String::new();
        let mut closed = false;
        while let Some(character) = chars.next() {
            if character == '}' {
                closed = true;
                break;
            }
            name.push(character);
        }
        if closed && crate::variables::is_valid_identifier(&name) {
            append_parameter(&name, current, word_started, glob, quoted);
        } else {
            current.push_str("${");
            current.push_str(&name);
            if closed {
                current.push('}');
            }
            *word_started = true;
        }
        return;
    }

    let mut name = String::new();
    while let Some(&character) = chars.peek() {
        let valid = if name.is_empty() {
            character.is_ascii_alphabetic() || character == '_'
        } else {
            character.is_ascii_alphanumeric() || character == '_'
        };
        if !valid {
            break;
        }
        name.push(character);
        chars.next();
    }

    if name.is_empty() {
        current.push('$');
        *word_started = true;
        return;
    }

    append_parameter(&name, current, word_started, glob, quoted);
}

struct RawWord {
    text: String,
    glob: bool,
}

fn finish_word(words: &mut Vec<RawWord>, current: &mut String, glob: &mut bool) {
    words.push(RawWord {
        text: std::mem::take(current),
        glob: *glob,
    });
    *glob = false;
}

fn insert_captured(
    text: &str,
    quoted: bool,
    words: &mut Vec<RawWord>,
    current: &mut String,
    glob: &mut bool,
    word_started: &mut bool,
) {
    let text = text.trim_end_matches(['\n', '\r']);
    if quoted {
        current.push_str(text);
        *word_started = true;
        return;
    }

    let fields: Vec<&str> = text
        .split(|character: char| matches!(character, ' ' | '\t' | '\n' | '\r'))
        .filter(|field| !field.is_empty())
        .collect();
    if fields.is_empty() {
        return;
    }

    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            finish_word(words, current, glob);
        }
        if contains_glob(field) {
            *glob = true;
        }
        current.push_str(field);
        *word_started = true;
    }
}

fn read_command_substitution(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut script = String::new();
    let mut depth = 1;
    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;
    while let Some(character) = chars.next() {
        if escaped {
            script.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if !in_single_quotes => {
                script.push(character);
                escaped = true;
            }
            '\'' if !in_double_quotes => {
                in_single_quotes = !in_single_quotes;
                script.push(character);
            }
            '"' if !in_single_quotes => {
                in_double_quotes = !in_double_quotes;
                script.push(character);
            }
            '(' if !in_single_quotes && !in_double_quotes => {
                depth += 1;
                script.push(character);
            }
            ')' if !in_single_quotes && !in_double_quotes => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                script.push(character);
            }
            _ => script.push(character),
        }
    }
    script
}

fn read_backtick(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut script = String::new();
    while let Some(character) = chars.next() {
        if character == '`' {
            break;
        }
        if character == '\\'
            && let Some(next) = chars.next()
        {
            if matches!(next, '$' | '`' | '\\') {
                script.push(next);
            } else {
                script.push('\\');
                script.push(next);
            }
            continue;
        }
        script.push(character);
    }
    script
}

pub(crate) fn parse_command(command: &str) -> ParsedCommand {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut current_glob = false;

    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;
    let mut word_started = false;

    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
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

            '`' if !in_single_quotes => {
                let script = read_backtick(&mut chars);
                let output = crate::execution::capture(&script);
                insert_captured(
                    &output,
                    in_double_quotes,
                    &mut words,
                    &mut current,
                    &mut current_glob,
                    &mut word_started,
                );
            }

            '$' if !in_single_quotes => {
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let script = read_command_substitution(&mut chars);
                    let output = crate::execution::capture(&script);
                    insert_captured(
                        &output,
                        in_double_quotes,
                        &mut words,
                        &mut current,
                        &mut current_glob,
                        &mut word_started,
                    );
                } else {
                    expand_dollar(
                        &mut chars,
                        &mut current,
                        &mut word_started,
                        &mut current_glob,
                        in_double_quotes,
                    );
                }
            }

            c if c.is_whitespace() && !in_single_quotes && !in_double_quotes => {
                if word_started {
                    finish_word(&mut words, &mut current, &mut current_glob);
                    word_started = false;
                }
            }

            _ => {
                if !in_single_quotes && !in_double_quotes && matches!(c, '*' | '?' | '[') {
                    current_glob = true;
                }
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
        finish_word(&mut words, &mut current, &mut current_glob);
    }

    let parts = expand_globs(words);

    let mut args = Vec::new();
    let mut stdin_file = None;
    let mut stdout_file = None;
    let mut stdout_append = false;
    let mut stderr_file = None;
    let mut stderr_append = false;

    let mut i = 0;

    while i < parts.len() {
        match parts[i].as_str() {
            "<" | "0<" => {
                if i + 1 < parts.len() {
                    stdin_file = Some(parts[i + 1].clone());
                    i += 2;
                    continue;
                }
            }

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
        stdin_file,
        stdout_file,
        stdout_append,
        stderr_file,
        stderr_append,
    }
}

fn expand_globs(words: Vec<RawWord>) -> Vec<String> {
    let mut parts = Vec::new();
    for word in words {
        if word.glob {
            let matches = glob_paths(&word.text);
            if matches.is_empty() {
                parts.push(word.text);
            } else {
                parts.extend(matches);
            }
        } else {
            parts.push(word.text);
        }
    }
    parts
}

fn glob_paths(pattern: &str) -> Vec<String> {
    let absolute = pattern.starts_with('/');
    let components: Vec<&str> = pattern
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    if components.is_empty() {
        return Vec::new();
    }

    let mut paths = vec![if absolute {
        "/".to_string()
    } else {
        String::new()
    }];
    for component in components {
        let mut next = Vec::new();
        for prefix in &paths {
            if !contains_glob(component) {
                next.push(join_path(prefix, component));
                continue;
            }
            let directory = if prefix.is_empty() {
                std::path::Path::new(".")
            } else {
                std::path::Path::new(prefix)
            };
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            for name in names {
                if name.starts_with('.') && !component.starts_with('.') {
                    continue;
                }
                if glob_match(component, &name) {
                    next.push(join_path(prefix, &name));
                }
            }
        }
        paths = next;
        if paths.is_empty() {
            return Vec::new();
        }
    }

    paths.retain(|path| std::path::Path::new(path).exists());
    paths.sort();
    paths
}

fn join_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else if prefix == "/" {
        format!("/{name}")
    } else {
        format!("{prefix}/{name}")
    }
}

fn glob_match(pattern: &str, name: &str) -> bool {
    glob_match_chars(
        &pattern.chars().collect::<Vec<_>>(),
        &name.chars().collect::<Vec<_>>(),
    )
}

fn glob_match_chars(pattern: &[char], name: &[char]) -> bool {
    match pattern.first() {
        None => name.is_empty(),
        Some('*') => {
            glob_match_chars(&pattern[1..], name)
                || (!name.is_empty() && glob_match_chars(pattern, &name[1..]))
        }
        Some('?') => !name.is_empty() && glob_match_chars(&pattern[1..], &name[1..]),
        Some('[') => {
            let Some(end) = pattern.iter().position(|character| *character == ']') else {
                return name.first() == Some(&'[') && glob_match_chars(&pattern[1..], &name[1..]);
            };
            if name.is_empty() {
                return false;
            }
            let mut class = &pattern[1..end];
            let negated = matches!(class.first().copied(), Some('!' | '^'));
            if negated {
                class = &class[1..];
            }
            if class_contains(class, name[0]) == negated {
                return false;
            }
            glob_match_chars(&pattern[end + 1..], &name[1..])
        }
        Some(character) => {
            name.first() == Some(character) && glob_match_chars(&pattern[1..], &name[1..])
        }
    }
}

fn class_contains(class: &[char], candidate: char) -> bool {
    let mut index = 0;
    while index < class.len() {
        if index + 2 < class.len() && class[index + 1] == '-' {
            if candidate >= class[index] && candidate <= class[index + 2] {
                return true;
            }
            index += 3;
        } else {
            if class[index] == candidate {
                return true;
            }
            index += 1;
        }
    }
    false
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
