const PREFIX_MAP: &[(&str, &str)] = &[
    ("apps/canopus/", "canopus"),
    ("apps/europa/", "europa"),
    ("laniakea/", "laniakea"),
    ("kepler/", "kepler"),
    ("dysonsphere/", "dysonsphere"),
    ("ton618/", "ton618"),
    ("hubble/", "hubble"),
    ("docs/", "docs"),
    ("scripts/", "scripts"),
];

pub fn derive_modules(changed_files: &[std::path::PathBuf]) -> Vec<String> {
    if changed_files.is_empty() {
        return Vec::new();
    }

    let mut modules: Vec<String> = changed_files
        .iter()
        .map(|path| {
            let path_str = path.to_string_lossy();
            PREFIX_MAP
                .iter()
                .filter(|(prefix, _)| path_str.starts_with(prefix))
                .max_by_key(|(prefix, _)| prefix.len())
                .map(|(_, name)| name.to_string())
                .unwrap_or_else(|| "misc".to_string())
        })
        .collect();

    modules.sort();
    modules.dedup();

    if modules.len() > 4 {
        modules.truncate(4);
        if !modules.contains(&"misc".to_string()) {
            modules.push("misc".to_string());
        }
    }

    modules
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn single_module() {
        assert_eq!(
            derive_modules(&[PathBuf::from("apps/canopus/src/x.rs")]),
            vec!["canopus"]
        );
    }

    #[test]
    fn multi_module_sorted() {
        assert_eq!(
            derive_modules(&[
                PathBuf::from("apps/canopus/x.rs"),
                PathBuf::from("laniakea/y.rs")
            ]),
            vec!["canopus", "laniakea"]
        );
    }

    #[test]
    fn unmapped_path() {
        assert_eq!(
            derive_modules(&[PathBuf::from("unknown/x.rs")]),
            vec!["misc"]
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(derive_modules(&[]), Vec::<String>::new());
    }

    #[test]
    fn dedup() {
        assert_eq!(
            derive_modules(&[
                PathBuf::from("apps/canopus/a.rs"),
                PathBuf::from("apps/canopus/b.rs")
            ]),
            vec!["canopus"]
        );
    }

    #[test]
    fn truncation_five_unique_modules() {
        let files = vec![
            PathBuf::from("apps/canopus/x.rs"),
            PathBuf::from("apps/europa/x.rs"),
            PathBuf::from("laniakea/x.rs"),
            PathBuf::from("kepler/x.rs"),
            PathBuf::from("dysonsphere/x.rs"),
        ];
        let result = derive_modules(&files);
        assert_eq!(result.len(), 5);
        assert!(result.contains(&"misc".to_string()));
        // first 4 are the lexicographic top-4 of the 5 unique names
        let top4: Vec<&str> = result
            .iter()
            .filter(|m| m.as_str() != "misc")
            .map(|s| s.as_str())
            .collect();
        assert_eq!(top4.len(), 4);
    }

    #[test]
    fn mixed_mapped_and_unmapped() {
        let files = vec![
            PathBuf::from("apps/canopus/x.rs"),
            PathBuf::from("unknown/y.rs"),
        ];
        let result = derive_modules(&files);
        assert!(result.contains(&"canopus".to_string()));
        assert!(result.contains(&"misc".to_string()));
    }
}
