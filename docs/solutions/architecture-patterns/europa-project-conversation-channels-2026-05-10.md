---
title: Europa Project Conversation Channels Stay Conversation-Only
date: 2026-05-10
category: architecture-patterns
module: apps/europa
problem_type: architecture_pattern
component: assistant
severity: medium
applies_when:
  - "Adding Discord project channels that should answer questions without creating Stellaris tasks"
  - "Reusing ASK_COMMAND for project-local assistant-style responses"
  - "Separating conversation surfaces from task, agenda, GitHub intake, or Canopus mutation paths"
related_components:
  - development_workflow
  - tooling
  - documentation
  - testing_framework
tags:
  - europa
  - discord
  - ask-command
  - conversation-only
  - project-context
  - mutation-boundary
---

# Europa Project Conversation Channels Stay Conversation-Only

## Context

Europa needed project-scoped Discord channels for analysis and brainstorming without turning every conversation into a durable Canopus work item. The V1 requirement was intentionally narrow: add `#analysis` and `#brainstorming` as registered project conversation surfaces, reuse the existing `ASK_COMMAND` backend, pass enough project context for useful answers, and avoid task, agenda, GitHub intake, or Canopus mutation paths.

The completed implementation adds:

- `#analysis` and `#brainstorming` to new project categories.
- `!analyze <topic>` locked to `#analysis`.
- `!brainstorm <topic>` locked to `#brainstorming`.
- Project-local backend context through prompt header, environment variables, and subprocess `cwd`.
- Guidance when `!run` is used in conversation-only channels instead of silently creating a task.

The non-goal is the important boundary: this is not a new Canopus daemon, agent loop, planner/coder/reviewer stage, task queue path, agenda writer, GitHub intake path, or protected mutation path.

## Guidance

When Europa needs lightweight project conversation surfaces, model them as channel-locked `ASK_COMMAND` calls with project context. Do not route them through the task pipeline unless the command is meant to create durable work.

Keep the channel vocabulary explicit:

```python
PROJECT_CHANNELS = (
    "general",
    "planning",
    "development",
    "review",
    "analysis",
    "brainstorming",
)
```

Conversation channels should stay out of the Canopus task role map:

```python
CHANNEL_TYPE_MAP = {
    "planning": "canopus.planner",
    "development": "canopus.agent",
    "review": "canopus.reviewer",
    "general": None,
    "analysis": None,
    "brainstorming": None,
}
```

Use one shared command helper for the guard sequence:

1. Authorize the Discord user.
2. Require a topic.
3. Require the exact conversation channel.
4. Require a Discord category.
5. Require a registered project for that category.
6. Build a prompt with project name, repo path, channel, role surface, role instruction, and user request.
7. Invoke `run_ask_backend` with project `cwd`, project metadata env, and the command label.

The backend context should be redundant on purpose: send it in stdin / `STELLARIS_ASK_PROMPT`, set `cwd` to the registered repository, and expose structured metadata as environment variables:

```python
answer, error = await run_ask_backend(
    prompt,
    cwd=project.get("repo_path"),
    extra_env={
        "STELLARIS_PROJECT_NAME": str(project.get("name", "")),
        "STELLARIS_PROJECT_REPO_PATH": str(project.get("repo_path", "")),
        "STELLARIS_DISCORD_CHANNEL": f"#{channel_name}",
        "STELLARIS_CONVERSATION_ROLE": conversation_role,
    },
    command_label=command_label,
)
```

Keep `!ask` separate and universal. It should continue to call `run_ask_backend(question)` without project, category, or channel guards so plain ad hoc questions are not forced into a project workflow.

## Why This Matters

This preserves the Stellaris responsibility boundary:

- Discord / Europa owns operator UI, channel routing, and response formatting.
- Canopus owns policy, state mutation, finalization, and workflow execution.
- Dysonsphere owns shared task contracts.

Without a conversation-only pattern, adding analyst-style or brainstormer-style commands can accidentally blur “talk about the project” into “create or mutate project work.” That creates ambiguous operator expectations and makes tests harder: a read-only discussion command should not need to prove Canopus finalization, GitHub intake, or task queue behavior.

The pattern also keeps `ASK_COMMAND` operator-owned. Europa provides consistent context and enforces its own no-mutation path, but the configured backend remains responsible for how it answers.

## When to Apply

Apply this pattern when:

- A Discord command should answer a project-local question without creating a durable task.
- The command needs repository context but should remain read-only from Europa's perspective.
- The interaction is role-flavored, such as analyst-style or brainstormer-style, but is not the real Canopus workflow stage.
- Existing project categories need new non-mutating surfaces.
- The change should stay out of `apps/canopus` and `apps/europa/payloads.py`.

Do not apply this pattern when:

- The command should create or mutate tasks.
- The command should enter approval or finalization.
- The command needs Canopus policy enforcement or protected git operations.
- The command should create GitHub issues, agendas, payload sidecars, branches, commits, PRs, merges, or deployments.

## Examples

Correct `#analysis` flow:

```text
!analyze where should retry handling live?
```

Europa should require a registered project category, build a project-context prompt, run `ASK_COMMAND` from the registered repository path, and send the answer back to Discord. It should not write `tasks.json`, create an agenda, call GitHub intake, or invoke Canopus.

Correct `#brainstorming` flow:

```text
!brainstorm alternatives for review handoff UX
```

Europa should use brainstormer-style instructions and the same project context mechanism, while still remaining a conversation-only path.

Incorrect `#analysis` flow:

```text
!run implement retry handling
```

In `#analysis`, `!run` should return guidance to use `!analyze <topic>`. Task creation remains limited to `#planning`, `#development`, and `#review`.

## Tests and Verification

The regression tests should prove the boundary, not just the happy path:

- Channel type mapping includes `analysis` and `brainstorming` as `None` task types.
- `!new-project` creates all six project channels.
- `run_ask_backend` preserves plain `!ask` defaults and accepts contextual `cwd`, env, and command labels for project conversation commands.
- Wrong-channel `!analyze` / `!brainstorm` calls do not call the backend, GitHub intake, or task append paths.
- Unregistered project categories reject project conversation commands.
- Successful project conversation commands pass prompt, `cwd`, and env context and do not create task files.
- `!ask` remains universal even in `#analysis` without a category.
- `!run` in conversation channels returns guidance and does not write tasks.

Fresh verification from the implementation:

```bash
python3 -m unittest apps/europa/test_bot_config.py
python3 -m py_compile apps/europa/europa.py apps/europa/canopus_client.py apps/europa/config.py apps/europa/test_bot_config.py
git diff --check
git diff --name-only | rg '^(apps/canopus/|apps/europa/payloads.py)' || true
```

The first command passed with 51 tests. The compile and diff checks passed. The boundary grep produced no output, confirming the change stayed out of Canopus and payload generation.

## Related

- `apps/europa/europa.py` — project channel list, conversation prompt construction, `!analyze`, `!brainstorm`, and `!run` conversation-channel guardrails.
- `apps/europa/canopus_client.py` — `run_ask_backend(..., cwd=None, extra_env=None, command_label="!ask")` project context support.
- `apps/europa/config.py` — conversation channels mapped to no task type.
- `apps/europa/test_bot_config.py` — command, context, and no-mutation regression coverage.
- `apps/europa/README.md` — operator-facing command and `ASK_COMMAND` documentation.
- `docs/v1-operator-runbook.md` — migration steps for adding `#analysis` and `#brainstorming` to existing project categories.
- `docs/solutions/logic-errors/approval-finalize-without-watch-daemon-2026-05-10.md` — related Discord/Canopus boundary lesson for approval-triggered mutation.
- `docs/solutions/architecture-patterns/canopus-capability-aware-pre-run-helpers-2026-05-10.md` — related pattern for policy-owned context helpers that do not let raw Discord text invoke deeper system behavior.
