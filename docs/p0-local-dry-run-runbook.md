# P0 Local Dry-Run Runbook

This runbook proves the local self-hosting loop without live mutation.

## Environment

Use a deterministic command runtime for local evidence:

```bash
export CANOPUS_AGENT_RUNTIME=command
export CANOPUS_AGENT_COMMAND='python3 -c "import os; print(\"canopus dry-run stage=\" + os.environ.get(\"CANOPUS_ROLE\", \"unknown\"))"'
export CANOPUS_ENABLE_GITHUB=0
export CANOPUS_ALLOW_GITHUB_MUTATION=0
export CANOPUS_ENABLE_LIVE_MUTATIONS=0
export CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=0
```

The command runs locally for each Canopus role stage and writes inspectable runtime artifacts. Keep `CANOPUS_AGENT_RUNTIME` unset to use the mock runtime.

## Launcher inspection

From the parent checkout directory:

```bash
pwsh Stellaris/start-pipeline.ps1 -DryRun
```

Expected dry-run output lists:

- TON618 using `TASKS_JSON_PATH`
- Laniakea using the same file-backed queue
- Canopus watch/finalizer using `<repo>/.canopus`
- Kepler
- Discord Bot

The dry-run path must not require GitHub or Discord credentials and must not reach push, PR creation, GitHub Issue mutation, or GitHub Project mutation.

## Finalizer behavior

When approval marks a task `Processed`, run or keep running:

```bash
CANOPUS_REPO=. CANOPUS_STATE=.canopus canopus watch tasks.json
```

The P0 completion artifact is:

```text
.canopus/runs/<task-id>-finalize.txt
```

A second watch tick should skip an already finalized task instead of rewriting the finalize record.
