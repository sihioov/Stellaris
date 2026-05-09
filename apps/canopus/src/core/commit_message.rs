/// Formats a conventional commit message for Canopus-generated commits.
///
/// # Arguments
/// * `reviewer_summary` — reserved for V2 (unused in output); pass `""` for now
/// * `modules` — list of affected modules; empty → `"misc"`
/// * `commit_type` — one of `feat|fix|refactor|docs|chore|style|test`
/// * `summary` — short description (caller controls casing; convention: lowercase, no period)
/// * `body` — free-form body; multiple paragraphs allowed
/// * `user_request` — original user request; backticks and double-quotes are escaped
/// * `runtime` — agent runtime identifier (e.g. `"codex"`)
/// * `model` — model identifier (e.g. `"gpt-5.5"`)
///
/// # Output format
/// ```text
/// [{module1/module2/...}] {commit_type}: {summary}
///
/// {body}
///
/// User-Request: !run {escaped_user_request}
///
/// Co-Authored-By: Canopus <noreply@stellaris.local>
/// Agent-Runtime: {runtime} ({model})
/// ```
#[allow(unused_variables)]
pub fn format_commit_message(
    reviewer_summary: &str,
    modules: &[String],
    commit_type: &str,
    summary: &str,
    body: &str,
    user_request: &str,
    runtime: &str,
    model: &str,
) -> String {
    // Module bracket: join with '/' or fall back to "misc"
    let module_str = if modules.is_empty() {
        "misc".to_string()
    } else {
        modules.join("/")
    };

    // Escape user_request: backtick → \` and double-quote → \"
    let escaped = user_request
        .replace('`', r"\`")
        .replace('"', "\\\"");

    // Multi-line user_request: first line after "!run ", subsequent lines indented 2 spaces
    let user_request_line = if escaped.contains('\n') {
        let mut parts = escaped.splitn(2, '\n');
        let first = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("");
        // Indent each subsequent line with two spaces
        let indented_rest: String = rest
            .lines()
            .map(|l| format!("  {}", l))
            .collect::<Vec<_>>()
            .join("\n");
        format!("User-Request: !run {}\n{}", first, indented_rest)
    } else {
        format!("User-Request: !run {}", escaped)
    };

    format!(
        "[{module}] {ctype}: {summary}\n\n{body}\n\n{user_req}\n\nCo-Authored-By: Canopus <noreply@stellaris.local>\nAgent-Runtime: {runtime} ({model})",
        module = module_str,
        ctype = commit_type,
        summary = summary,
        body = body,
        user_req = user_request_line,
        runtime = runtime,
        model = model,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(modules: &[&str], commit_type: &str, summary: &str, body: &str, user_request: &str) -> String {
        let mods: Vec<String> = modules.iter().map(|s| s.to_string()).collect();
        format_commit_message("", &mods, commit_type, summary, body, user_request, "codex", "gpt-5.5")
    }

    #[test]
    fn test_subject_regex_match() {
        let out = make(&["canopus"], "feat", "add something", "body here", "request");
        let first_line = out.lines().next().unwrap();
        // Regex: ^\[[a-z0-9/_-]+\]\s+(feat|fix|refactor|docs|chore|style|test):\s+\S
        assert!(first_line.starts_with('['));
        let close = first_line.find(']').expect("closing bracket");
        let bracket_content = &first_line[1..close];
        assert!(bracket_content.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-'));
        let after = &first_line[close + 1..];
        assert!(after.starts_with(' '));
        let valid_types = ["feat", "fix", "refactor", "docs", "chore", "style", "test"];
        let found = valid_types.iter().any(|t| after.trim_start().starts_with(&format!("{}:", t)));
        assert!(found, "subject line did not match expected commit type pattern: {}", first_line);
    }

    #[test]
    fn test_multi_module_bracket() {
        let mods: Vec<String> = vec!["canopus".into(), "laniakea".into()];
        let out = format_commit_message("", &mods, "feat", "add routing", "body", "req", "codex", "gpt-5.5");
        assert!(out.lines().next().unwrap().starts_with("[canopus/laniakea] "));
    }

    #[test]
    fn test_empty_modules_misc() {
        let out = make(&[], "chore", "update deps", "body", "req");
        assert!(out.lines().next().unwrap().starts_with("[misc] "));
    }

    #[test]
    fn test_trailer_order() {
        let out = make(&["canopus"], "fix", "fix thing", "body", "req");
        let ur = out.find("User-Request:").expect("User-Request not found");
        let ca = out.find("Co-Authored-By:").expect("Co-Authored-By not found");
        let ar = out.find("Agent-Runtime:").expect("Agent-Runtime not found");
        assert!(ur < ca, "User-Request must come before Co-Authored-By");
        assert!(ca < ar, "Co-Authored-By must come before Agent-Runtime");
    }

    #[test]
    fn test_backtick_escape() {
        let out = make(&["canopus"], "fix", "fix null", "body", "fix `null` check");
        // escaped form: \`null\`
        assert!(out.contains(r"User-Request: !run fix \`null\` check"), "output: {}", out);
    }

    #[test]
    fn test_quote_escape() {
        let out = make(&["canopus"], "fix", "say hi", "body", r#"say "hi""#);
        assert!(out.contains(r#"User-Request: !run say \"hi\""#), "output: {}", out);
    }

    #[test]
    fn test_multiline_user_request() {
        let out = make(&["canopus"], "feat", "multi", "body", "line1\nline2");
        assert!(out.contains("User-Request: !run line1\n  line2"), "output: {}", out);
    }

    #[test]
    fn test_body_rendering() {
        let body = "paragraph one\n\nparagraph two";
        let out = make(&["canopus"], "docs", "update docs", body, "req");
        assert!(out.contains("paragraph one\n\nparagraph two"));
    }

    #[test]
    fn test_trailer_values_present() {
        let mods: Vec<String> = vec!["canopus".into()];
        let out = format_commit_message("", &mods, "feat", "something", "body", "req", "codex", "gpt-5.5");
        assert!(out.contains("Co-Authored-By: Canopus <noreply@stellaris.local>"));
        assert!(out.contains("Agent-Runtime: codex (gpt-5.5)"));
    }

    #[test]
    fn test_blank_line_before_user_request() {
        let out = make(&["canopus"], "feat", "something", "body text", "req");
        assert!(out.contains("\n\nUser-Request:"), "output: {}", out);
    }
}
