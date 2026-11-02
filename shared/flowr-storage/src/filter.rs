//! SQL filters as A3T's flows write them.
//!
//! A3T's flows put string values in double quotes (`org_id = "org-default"`). The
//! LanceDB they were written for read those as strings; Lance 14 follows standard
//! SQL, where double quotes name a column. [`compat`] keeps the flows' meaning: a
//! double-quoted text outside single quotes becomes a single-quoted string (a `'`
//! inside it doubled). Single-quoted strings and backtick identifiers are kept.

/// The filter with double-quoted values turned into SQL strings.
pub fn compat(filter: &str) -> String {
    let mut out = String::with_capacity(filter.len() + 4);
    let mut chars = filter.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '`' => {
                // Copied as is, up to the matching close (`''` stays inside).
                out.push(c);
                while let Some(d) = chars.next() {
                    out.push(d);
                    if d == c {
                        if c == '\'' && chars.peek() == Some(&'\'') {
                            out.push(chars.next().unwrap());
                            continue;
                        }
                        break;
                    }
                }
            }
            '"' => {
                out.push('\'');
                while let Some(d) = chars.next() {
                    match d {
                        '"' => break,
                        '\'' => out.push_str("''"),
                        '\\' if chars.peek() == Some(&'"') => out.push(chars.next().unwrap()),
                        _ => out.push(d),
                    }
                }
                out.push('\'');
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::compat;

    #[test]
    fn double_quoted_values_become_strings() {
        assert_eq!(compat(r#"user_id = "usr-root""#), "user_id = 'usr-root'");
        assert_eq!(compat(r#"id = "a" AND org_id = "b""#), "id = 'a' AND org_id = 'b'");
        assert_eq!(compat(r#"name = "O'Brien""#), "name = 'O''Brien'");
        assert_eq!(compat(r#"note = "say \"hi\"""#), r#"note = 'say "hi"'"#);
    }

    #[test]
    fn single_quotes_and_identifiers_are_kept() {
        assert_eq!(compat("created_by = 'system'"), "created_by = 'system'");
        assert_eq!(compat("name = 'it''s \"x\"'"), "name = 'it''s \"x\"'");
        assert_eq!(compat("`my col` = 'v'"), "`my col` = 'v'");
        assert_eq!(compat("status IN ('a', 'b') AND n > 3"), "status IN ('a', 'b') AND n > 3");
        assert_eq!(compat(""), "");
    }
}
