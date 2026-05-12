use crate::core::{CanopusError, CanopusResult};
use dysonsphere::message::TaskType;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeCapability {
    Plan,
    Implement,
    Review,
}

impl RuntimeCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeCapability::Plan => "plan",
            RuntimeCapability::Implement => "implement",
            RuntimeCapability::Review => "review",
        }
    }

    pub fn requires_branch(self) -> bool {
        matches!(self, RuntimeCapability::Implement)
    }

    fn from_role_mode(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "planner" | "plan" => Some(RuntimeCapability::Plan),
            "agent" | "full" | "implement" | "implementation" | "standard" => {
                Some(RuntimeCapability::Implement)
            }
            "reviewer" | "review" => Some(RuntimeCapability::Review),
            _ => None,
        }
    }

    fn from_task_type(task_type: Option<&TaskType>) -> Option<Self> {
        match task_type {
            Some(TaskType::Custom(label)) => match label.as_str() {
                "canopus.planner" => Some(RuntimeCapability::Plan),
                "canopus.agent" => Some(RuntimeCapability::Implement),
                "canopus.reviewer" => Some(RuntimeCapability::Review),
                _ => None,
            },
            Some(TaskType::Bug)
            | Some(TaskType::Security)
            | Some(TaskType::TestCoverage)
            | Some(TaskType::UXImprovement) => Some(RuntimeCapability::Implement),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendSelection {
    pub capability: RuntimeCapability,
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub source: BackendSelectionSource,
    pub default_or_override_source: BackendSelectionSource,
    pub preparation: PreparationPolicy,
    pub read_only: bool,
    pub argv: Option<Vec<String>>,
    pub env_allowlist: Vec<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendSelectionAttempt {
    pub capability: Option<RuntimeCapability>,
    pub requested_backend: Option<String>,
    pub directive_source: BackendAttemptDirectiveSource,
    pub read_only: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    Legacy,
    Mock,
    Codex,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendSelectionSource {
    Default,
    Directive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendAttemptDirectiveSource {
    Default,
    Directive,
    InvalidDirective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreparationPolicy {
    Branch,
    ReadOnly,
}

#[derive(Debug, Clone)]
pub struct RuntimeRegistry {
    config: RegistryConfig,
}

impl RuntimeRegistry {
    pub fn from_env() -> CanopusResult<Self> {
        match std::env::var("CANOPUS_BACKEND_REGISTRY_CONFIG") {
            Ok(path) if !path.trim().is_empty() => Self::from_path(path),
            _ => Ok(Self {
                config: RegistryConfig::legacy_defaults(),
            }),
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> CanopusResult<Self> {
        let content = fs::read_to_string(path)?;
        let config: RegistryConfig = serde_json::from_str(&content).map_err(|err| {
            CanopusError::InvalidInput(format!("invalid backend registry config: {err}"))
        })?;
        config.validate()?;
        Ok(Self { config })
    }

    pub fn resolve(
        &self,
        role_mode: &str,
        task_type: Option<&TaskType>,
        request: &str,
    ) -> CanopusResult<BackendSelection> {
        let capability = resolve_capability(role_mode, task_type)?;
        let directive = parse_backend_directive(request)?;
        let (backend_name, source) = match directive {
            Some(name) => {
                self.ensure_override_allowed(capability, &name)?;
                (name, BackendSelectionSource::Directive)
            }
            None => (
                self.default_backend(capability)?.to_string(),
                BackendSelectionSource::Default,
            ),
        };
        let backend = self.config.backends.get(&backend_name).ok_or_else(|| {
            CanopusError::InvalidInput(format!("backend '{backend_name}' is not registered"))
        })?;
        let preparation = if capability.requires_branch() {
            PreparationPolicy::Branch
        } else {
            PreparationPolicy::ReadOnly
        };
        Ok(BackendSelection {
            capability,
            backend_name,
            backend_kind: backend.kind,
            source,
            default_or_override_source: source,
            preparation,
            read_only: preparation == PreparationPolicy::ReadOnly,
            argv: backend.argv.clone(),
            env_allowlist: backend.env_allowlist.clone(),
            timeout_seconds: backend.timeout_seconds,
        })
    }

    pub fn describe_attempt(
        &self,
        role_mode: &str,
        task_type: Option<&TaskType>,
        request: &str,
    ) -> BackendSelectionAttempt {
        let capability = resolve_capability(role_mode, task_type).ok();
        let (requested_backend, directive_source) = match parse_backend_directive(request) {
            Ok(Some(name)) => (Some(name), BackendAttemptDirectiveSource::Directive),
            Ok(None) => (None, BackendAttemptDirectiveSource::Default),
            Err(_) => (None, BackendAttemptDirectiveSource::InvalidDirective),
        };
        BackendSelectionAttempt {
            capability,
            requested_backend,
            directive_source,
            read_only: capability.map(|capability| !capability.requires_branch()),
        }
    }

    fn default_backend(&self, capability: RuntimeCapability) -> CanopusResult<&str> {
        let value = match capability {
            RuntimeCapability::Plan => &self.config.capability_defaults.plan,
            RuntimeCapability::Implement => &self.config.capability_defaults.implement,
            RuntimeCapability::Review => &self.config.capability_defaults.review,
        };
        value.as_deref().ok_or_else(|| {
            CanopusError::InvalidInput(format!(
                "no default backend configured for capability '{}'",
                capability.as_str()
            ))
        })
    }

    fn ensure_override_allowed(
        &self,
        capability: RuntimeCapability,
        backend_name: &str,
    ) -> CanopusResult<()> {
        let allowed = match capability {
            RuntimeCapability::Plan => &self.config.capability_override_allowlists.plan,
            RuntimeCapability::Implement => &self.config.capability_override_allowlists.implement,
            RuntimeCapability::Review => &self.config.capability_override_allowlists.review,
        };
        if allowed.iter().any(|name| name == backend_name) {
            Ok(())
        } else {
            Err(CanopusError::InvalidInput(format!(
                "backend '{backend_name}' is not allowed for capability '{}'",
                capability.as_str()
            )))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryConfig {
    backends: BTreeMap<String, BackendConfig>,
    #[serde(default)]
    capability_defaults: CapabilityDefaults,
    #[serde(default)]
    capability_override_allowlists: CapabilityAllowlists,
}

impl RegistryConfig {
    fn legacy_defaults() -> Self {
        let mut backends = BTreeMap::new();
        backends.insert(
            "legacy".to_string(),
            BackendConfig {
                kind: BackendKind::Legacy,
                argv: None,
                env_allowlist: Vec::new(),
                timeout_seconds: None,
            },
        );
        Self {
            backends,
            capability_defaults: CapabilityDefaults {
                plan: Some("legacy".to_string()),
                implement: Some("legacy".to_string()),
                review: Some("legacy".to_string()),
            },
            capability_override_allowlists: CapabilityAllowlists {
                plan: vec!["legacy".to_string()],
                implement: vec!["legacy".to_string()],
                review: vec!["legacy".to_string()],
            },
        }
    }

    fn validate(&self) -> CanopusResult<()> {
        if self.backends.is_empty() {
            return Err(CanopusError::InvalidInput(
                "backend registry must define at least one backend".to_string(),
            ));
        }
        for (name, backend) in &self.backends {
            validate_backend_name(name)?;
            if matches!(backend.kind, BackendKind::Command)
                && backend
                    .argv
                    .as_ref()
                    .map(|argv| argv.is_empty())
                    .unwrap_or(true)
            {
                return Err(CanopusError::InvalidInput(format!(
                    "command backend '{name}' requires non-empty argv"
                )));
            }
        }
        for name in self
            .capability_defaults
            .iter_names()
            .chain(self.capability_override_allowlists.iter_names())
        {
            if !self.backends.contains_key(name) {
                return Err(CanopusError::InvalidInput(format!(
                    "backend '{name}' is referenced but not registered"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct BackendConfig {
    kind: BackendKind,
    #[serde(default)]
    argv: Option<Vec<String>>,
    #[serde(default)]
    env_allowlist: Vec<String>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CapabilityDefaults {
    plan: Option<String>,
    implement: Option<String>,
    review: Option<String>,
}

impl CapabilityDefaults {
    fn iter_names(&self) -> impl Iterator<Item = &str> {
        [&self.plan, &self.implement, &self.review]
            .into_iter()
            .filter_map(|value| value.as_deref())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CapabilityAllowlists {
    #[serde(default)]
    plan: Vec<String>,
    #[serde(default)]
    implement: Vec<String>,
    #[serde(default)]
    review: Vec<String>,
}

impl CapabilityAllowlists {
    fn iter_names(&self) -> impl Iterator<Item = &str> {
        self.plan
            .iter()
            .chain(self.implement.iter())
            .chain(self.review.iter())
            .map(String::as_str)
    }
}

fn resolve_capability(
    role_mode: &str,
    task_type: Option<&TaskType>,
) -> CanopusResult<RuntimeCapability> {
    let role_mode_capability = RuntimeCapability::from_role_mode(role_mode);
    let task_type_capability = RuntimeCapability::from_task_type(task_type);
    match (role_mode_capability, task_type_capability) {
        (Some(left), Some(right)) if left != right => Err(CanopusError::InvalidInput(format!(
            "role_mode '{role_mode}' conflicts with task_type capability '{}'",
            right.as_str()
        ))),
        (Some(capability), _) | (_, Some(capability)) => Ok(capability),
        (None, None) => Ok(RuntimeCapability::Implement),
    }
}

fn parse_backend_directive(request: &str) -> CanopusResult<Option<String>> {
    let mut found: Option<String> = None;
    let mut in_fence = false;
    for line in request.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for token in line.split_whitespace() {
            let Some(value) = token.strip_prefix("backend=") else {
                continue;
            };
            validate_backend_name(value)?;
            if found.is_some() {
                return Err(CanopusError::InvalidInput(
                    "multiple backend directives are not allowed".to_string(),
                ));
            }
            found = Some(value.to_string());
        }
    }
    Ok(found)
}

fn validate_backend_name(name: &str) -> CanopusResult<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    {
        return Err(CanopusError::InvalidInput(format!(
            "invalid backend name '{name}'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directive_parser_rejects_duplicate_even_same_backend() {
        let err = parse_backend_directive("backend=a backend=a").unwrap_err();
        assert!(err.to_string().contains("multiple backend directives"));
    }

    #[test]
    fn directive_parser_ignores_fenced_code_blocks() {
        let parsed =
            parse_backend_directive("```text\nbackend=ignored\n```\nbackend=real").unwrap();
        assert_eq!(parsed.as_deref(), Some("real"));
    }

    #[test]
    fn capability_conflict_fails_closed() {
        let err = resolve_capability("planner", Some(&TaskType::Custom("canopus.agent".into())))
            .unwrap_err();
        assert!(err.to_string().contains("conflicts"));
    }
}
