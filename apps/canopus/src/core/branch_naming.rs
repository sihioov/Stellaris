/// Derives a git branch name from a user request string and a run ID.
///
/// Format: `canopus/{slug}-{short}`
/// - slug: first 5 ASCII-only lowercase words from `user_request`, joined with `-`,
///         falling back to `"task"` if no ASCII words remain
/// - short: first 6 hex characters of `run_id`, zero-padded to length 6
pub fn derive_branch_name(user_request: &str, run_id: &str) -> String {
    // Step 1: lowercase and drop non-ASCII chars
    let ascii_only: String = user_request
        .chars()
        .filter(|c| c.is_ascii())
        .collect::<String>()
        .to_lowercase();

    // Step 2: split on whitespace, take first 5 words, strip non-[a-z0-9] from each word
    let words: Vec<String> = ascii_only
        .split_whitespace()
        .take(5)
        .map(|w| w.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect();

    let slug = if words.is_empty() {
        "task".to_string()
    } else {
        words.join("-")
    };

    // Step 3: derive short run_id — first 6 hex chars of run_id, pad with '0' if needed
    let hex_chars: String = run_id
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(6)
        .collect();
    let short = format!("{:0<6}", hex_chars);

    format!("canopus/{}-{}", slug, short)
}

/// Appends `-{n}` to `base` for collision avoidance.
///
/// Example: `with_collision_suffix("canopus/x-abc123", 2)` → `"canopus/x-abc123-2"`
pub fn with_collision_suffix(base: &str, n: u32) -> String {
    format!("{}-{}", base, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_command() {
        assert_eq!(
            derive_branch_name("add a readme", "a3f9c2deadbeef"),
            "canopus/add-a-readme-a3f9c2"
        );
    }

    #[test]
    fn test_mixed_korean_ascii() {
        // "README" survives ASCII filter; Korean chars drop
        let result = derive_branch_name("README 만들어줘", "a3f9c2");
        assert!(result.starts_with("canopus/"));
        assert!(result.ends_with("-a3f9c2"));
        // "readme" survives from "README" → slug is "readme"
        assert!(result.contains("readme") || result.contains("task"));
    }

    #[test]
    fn test_more_than_five_words_truncated() {
        assert_eq!(
            derive_branch_name("one two three four five six", "abc123"),
            "canopus/one-two-three-four-five-abc123"
        );
    }

    #[test]
    fn test_special_chars_stripped() {
        assert_eq!(
            derive_branch_name("fix `null` check!", "abc123"),
            "canopus/fix-null-check-abc123"
        );
    }

    #[test]
    fn test_collision_suffix() {
        assert_eq!(
            with_collision_suffix("canopus/x-abc123", 2),
            "canopus/x-abc123-2"
        );
    }

    #[test]
    fn test_pure_non_ascii_fallback() {
        assert_eq!(
            derive_branch_name("한글만", "deadbeef99"),
            "canopus/task-deadbe"
        );
    }

    #[test]
    fn test_short_run_id_padding() {
        assert_eq!(
            derive_branch_name("hello", "abc"),
            "canopus/hello-abc000"
        );
    }
}
