use crate::core::{CanopusError, CanopusResult, HelperOutput, HelperRequest, HelperSelection};
use crate::ports::PreRunHelperBackend;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, UNIX_EPOCH};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const WATCHED_IGNORED_PATHS: &[&str] = &[".omx", ".canopus", "target"];
const MAX_SNAPSHOT_ENTRIES: usize = 512;
const MAX_HASH_BYTES_PER_FILE: u64 = 64 * 1024;

#[derive(Debug, Clone)]
pub struct RepoExplorePreRunHelperBackend {
    max_output_bytes: usize,
    command_override: Option<Vec<String>>,
}

impl RepoExplorePreRunHelperBackend {
    pub fn new(max_output_bytes: usize) -> Self {
        Self {
            max_output_bytes,
            command_override: None,
        }
    }

    /// Test/support constructor for exercising command guard behavior without
    /// depending on a real `omx` binary. Production submit wiring uses `new`.
    pub fn with_command(max_output_bytes: usize, command: Vec<String>) -> Self {
        Self {
            max_output_bytes,
            command_override: Some(command),
        }
    }
}

impl PreRunHelperBackend for RepoExplorePreRunHelperBackend {
    fn identity(&self) -> String {
        self.command_override
            .as_ref()
            .map(|command| command.join(" "))
            .unwrap_or_else(|| "omx explore".to_string())
    }

    fn run(
        &self,
        repo: &Path,
        request: &HelperRequest,
        selection: &HelperSelection,
    ) -> CanopusResult<HelperOutput> {
        let before = MutationSnapshot::capture(repo)?;
        let prompt = repo_explore_prompt(request, selection);
        let mut command = self.command_override.clone().unwrap_or_else(|| {
            vec![
                "omx".to_string(),
                "explore".to_string(),
                "--prompt".to_string(),
            ]
        });
        if self.command_override.is_none() {
            command.push(prompt);
        }
        let output = run_command(repo, &command, DEFAULT_TIMEOUT)?;
        let after = MutationSnapshot::capture(repo)?;
        if before != after {
            return Err(CanopusError::Runtime(
                "read-only guard failed: pre-run helper mutated tracked, untracked, or ignored repository paths".to_string(),
            ));
        }
        if output.status != 0 {
            return Err(CanopusError::Runtime(format!(
                "pre-run helper `{}` exited with status {}\nstdout:\n{}\nstderr:\n{}",
                selection.name, output.status, output.stdout, output.stderr
            )));
        }

        let mut content = output.stdout;
        if !output.stderr.trim().is_empty() {
            content.push_str("\n[stderr]\n");
            content.push_str(&output.stderr);
        }
        let (content, truncated) = truncate(content, self.max_output_bytes);
        Ok(HelperOutput {
            summary: format!("{} completed via {}", selection.name, self.identity()),
            content,
            truncated,
            read_only_check: "passed: pre/post mutation snapshot unchanged".to_string(),
        })
    }
}

fn repo_explore_prompt(request: &HelperRequest, selection: &HelperSelection) -> String {
    format!(
        "Summarize repository-local context for Canopus `{}` stage before role `{}`. \
Focus on files, symbols, tests, and risks relevant to this task. Do not edit files. \
Helper reason: {}. User request summary: {}",
        request.stage_name,
        request.role.as_str(),
        selection.reason,
        request.user_request_summary
    )
}

#[derive(Debug)]
struct CommandResult {
    status: i32,
    stdout: String,
    stderr: String,
}

fn run_command(repo: &Path, command: &[String], timeout: Duration) -> CanopusResult<CommandResult> {
    if command.is_empty() {
        return Err(CanopusError::InvalidInput(
            "pre-run helper command must not be empty".to_string(),
        ));
    }
    if command[0] != "omx" {
        return Err(CanopusError::InvalidInput(format!(
            "pre-run helper command is not allowlisted: {}",
            command[0]
        )));
    }

    let isolated = std::env::temp_dir().join(format!(
        "canopus-helper-state-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::create_dir_all(&isolated)?;

    let mut child = match Command::new(&command[0])
        .args(&command[1..])
        .current_dir(repo)
        .env("OMX_STATE_DIR", isolated.join("state"))
        .env("OMX_CACHE_DIR", isolated.join("cache"))
        .env("OMX_LOG_DIR", isolated.join("logs"))
        .env("OMX_RUNTIME_DIR", isolated.join("runtime"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let _ = fs::remove_dir_all(&isolated);
            return Err(err.into());
        }
    };
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            let output = child.wait_with_output()?;
            let _ = fs::remove_dir_all(&isolated);
            return Ok(CommandResult {
                status: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            let _ = fs::remove_dir_all(&isolated);
            return Err(CanopusError::Runtime(format!(
                "pre-run helper timed out after {}s\nstdout:\n{}\nstderr:\n{}",
                timeout.as_secs(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn truncate(mut content: String, max_bytes: usize) -> (String, bool) {
    if content.len() <= max_bytes {
        return (content, false);
    }
    let mut boundary = max_bytes;
    while !content.is_char_boundary(boundary) {
        boundary -= 1;
    }
    content.truncate(boundary);
    content.push_str("\n[truncated]\n");
    (content, true)
}

#[derive(Debug, PartialEq, Eq)]
struct MutationSnapshot {
    git_status: String,
    watched_paths: Vec<String>,
}

impl MutationSnapshot {
    fn capture(repo: &Path) -> CanopusResult<Self> {
        let git_status = Command::new("git")
            .args(["status", "--porcelain=v1", "--ignored"])
            .current_dir(repo)
            .output()?;
        let mut watched_paths = Vec::new();
        for relative in WATCHED_IGNORED_PATHS {
            let path = repo.join(relative);
            if path.exists() {
                collect_path(repo, &path, &mut watched_paths, MAX_SNAPSHOT_ENTRIES)?;
            }
        }
        watched_paths.sort();
        Ok(Self {
            git_status: format!(
                "status={}\nstdout={}\nstderr={}",
                git_status.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&git_status.stdout),
                String::from_utf8_lossy(&git_status.stderr)
            ),
            watched_paths,
        })
    }
}

fn collect_path(
    repo: &Path,
    path: &Path,
    entries: &mut Vec<String>,
    max_entries: usize,
) -> CanopusResult<()> {
    if entries.len() >= max_entries {
        entries.push("<snapshot-entry-limit-reached>".to_string());
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    let rel = path
        .strip_prefix(repo)
        .unwrap_or(path)
        .display()
        .to_string();
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| format!("{}.{}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_else(|| "unknown".to_string());
    if metadata.is_dir() {
        entries.push(format!("dir:{rel}:modified={modified}"));
        let mut children = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(|entry| entry.path());
        for child in children {
            collect_path(repo, &child.path(), entries, max_entries)?;
            if entries.len() >= max_entries {
                break;
            }
        }
    } else if metadata.is_file() {
        entries.push(format!(
            "file:{rel}:len={}:modified={modified}:hash={}",
            metadata.len(),
            hash_file(path, MAX_HASH_BYTES_PER_FILE)?
        ));
    } else {
        entries.push(format!("other:{rel}:modified={modified}"));
    }
    Ok(())
}

fn hash_file(path: &Path, max_bytes: u64) -> CanopusResult<u64> {
    let mut file = fs::File::open(path)?;
    let mut hasher = DefaultHasher::new();
    let mut remaining = max_bytes;
    let mut buffer = [0u8; 8192];
    while remaining > 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let read = file.read(&mut buffer[..limit])?;
        if read == 0 {
            break;
        }
        buffer[..read].hash(&mut hasher);
        remaining -= read as u64;
    }
    Ok(hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AgentRole, HelperSelection, PreRunHelperMode};
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::{LazyLock, Mutex, MutexGuard};

    static PATH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct PathGuard {
        _guard: MutexGuard<'static, ()>,
        old_path: Option<String>,
    }

    impl PathGuard {
        fn prepend(path: &Path) -> Self {
            let guard = PATH_LOCK.lock().unwrap();
            let old_path = std::env::var("PATH").ok();
            let suffix = old_path.clone().unwrap_or_default();
            std::env::set_var("PATH", format!("{}:{suffix}", path.display()));
            Self {
                _guard: guard,
                old_path,
            }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            match &self.old_path {
                Some(old_path) => std::env::set_var("PATH", old_path),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    fn git_repo(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("canopus-helper-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Command::new("git")
            .arg("init")
            .current_dir(&root)
            .output()
            .unwrap();
        fs::write(root.join("README.md"), "# fixture\n").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&root)
            .output()
            .unwrap();
        root
    }

    fn request() -> HelperRequest {
        HelperRequest {
            agenda_id: "agenda".to_string(),
            role_task_id: "task".to_string(),
            role: AgentRole::Reviewer,
            stage_name: "review".to_string(),
            user_request_summary: "review request".to_string(),
            prior_artifact_count: 1,
        }
    }

    fn selection() -> HelperSelection {
        HelperSelection {
            name: "repo-explore".to_string(),
            mode: PreRunHelperMode::RepoExplore,
            reason: "test".to_string(),
            attach_as_context: true,
            required: false,
        }
    }

    fn fake_omx(repo: &Path, script: &str) -> PathBuf {
        let fake_bin = repo.join("fake-bin");
        fs::create_dir_all(&fake_bin).unwrap();
        let fake_omx = fake_bin.join("omx");
        fs::write(&fake_omx, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake_omx).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_omx, perms).unwrap();
        }
        fake_bin
    }

    #[test]
    fn command_backend_detects_ignored_path_mutation() {
        let repo = git_repo("ignored-mutation");
        let fake_bin = fake_omx(
            &repo,
            "#!/bin/sh\nmkdir -p .omx\nprintf mutation > .omx/mutated.txt\nprintf mutated\n",
        );
        let _path = PathGuard::prepend(&fake_bin);
        let backend = RepoExplorePreRunHelperBackend::with_command(1024, vec!["omx".to_string()]);

        let err = backend.run(&repo, &request(), &selection()).unwrap_err();

        assert!(err.to_string().contains("read-only guard failed"));
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn command_backend_allows_unchanged_dirty_baseline() {
        let repo = git_repo("dirty-baseline");
        fs::create_dir_all(repo.join(".omx")).unwrap();
        fs::write(repo.join(".omx/existing.txt"), "existing").unwrap();
        fs::write(repo.join("changed.txt"), "dirty").unwrap();
        let fake_bin = fake_omx(&repo, "#!/bin/sh\nprintf 'readonly output\\n'\n");
        let _path = PathGuard::prepend(&fake_bin);
        let backend = RepoExplorePreRunHelperBackend::with_command(1024, vec!["omx".to_string()]);

        let output = backend.run(&repo, &request(), &selection()).unwrap();

        assert!(output.content.contains("readonly output"));
        assert!(output.read_only_check.contains("unchanged"));
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn truncate_preserves_utf8_boundaries() {
        let (content, truncated) = truncate("한글 helper output".to_string(), 5);

        assert!(truncated);
        assert_eq!(content, "한\n[truncated]\n");
    }
}
