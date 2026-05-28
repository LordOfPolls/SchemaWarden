pub fn normalise_sql_text(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < chars.len() {
        // Line comment: skip everything until end of line
        if chars[i] == '-' && chars.get(i + 1) == Some(&'-') {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            if i < chars.len() {
                out.push('\n');
                i += 1;
            }
            continue;
        }

        // Block comment: skip content, supporting nesting (/* /* */ */)
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            let mut depth = 1u32;
            while i < chars.len() && depth > 0 {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if !out.ends_with(' ') && !out.ends_with('\n') && !out.is_empty() {
                out.push(' ');
            }
            continue;
        }

        // Quoted literals and bracket-escaped identifiers: copy verbatim, handle SQL doubling escapes
        let close = match chars[i] {
            '\'' => Some('\''),
            '"' => Some('"'),
            '[' => Some(']'),
            _ => None,
        };
        if let Some(close) = close {
            out.push(chars[i]);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == close {
                    if chars.get(i + 1) == Some(&close) {
                        // doubled closing char is an escape sequence, not end-of-literal
                        out.push(chars[i + 1]);
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }

        match chars[i] {
            '\r' => {}
            '\n' => out.push('\n'),
            ' ' | '\t' => {
                if !out.ends_with(' ') && !out.ends_with('\n') && !out.is_empty() {
                    out.push(' ');
                }
            }
            c => out.push(c),
        }

        i += 1;
    }

    out.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
