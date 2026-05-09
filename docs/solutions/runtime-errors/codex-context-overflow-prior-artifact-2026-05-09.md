---
title: Codex Runtime Context Overflow via artifact_content_for_role Returning runtime_log
date: 2026-05-09
category: runtime-errors
module: canopus/adapters/agent_runtime
problem_type: runtime_error
component: tooling
severity: high
symptoms:
  - "Input exceeds the maximum length of 1048576 characters. actual_chars:2412777"
  - Codex CLI crashes on plan, code, or review stages when prior_artifact is passed
  - Only analyst and coder roles trigger the crash; planner and reviewer stages complete successfully
  - Artifact size is ~2.4MB instead of the expected ~5KB polished response
root_cause: wrong_api
resolution_type: code_fix
tags:
  - codex
  - context-overflow
  - agent-runtime
  - prior-artifact
  - canopus
---

# Codex Runtime Context Overflow via artifact_content_for_role Returning runtime_log

## Problem

`artifact_content_for_role()` in the Codex agent runtime returned the full `runtime_log` (2.4MB) for analyst and coder roles instead of the lean `final_message` (~5KB). This caused Codex CLI to crash with an input length error whenever plan, code, or review stages received a prior artifact from an upstream analyst or coder stage.

## Symptoms

- `Input exceeds the maximum length of 1048576 characters. actual_chars:2412777` — Codex CLI hard crash during plan/code/review stages
- Only analyst and coder roles triggered the crash; planner and reviewer stages completed successfully
- Artifact stored on disk was ~2.4MB rather than the expected ~5KB polished response
- The bug was invisible in single-stage runs because only multi-stage pipelines pass `prior_artifact`

## What Didn't Work

- Initially suspected the planning prompt construction was too large — ruled out after inspecting prompt building; the prompt itself was within limits
- The asymmetric `match` branch was masked because Planner and Reviewer already used `final_message`, making early test runs with only those roles pass cleanly

## Solution

Single function change in `apps/canopus/src/adapters/agent_runtime/codex.rs` (lines 226–231):

```rust
// Before (bug): analyst/coder roles returned full 2.4MB runtime_log
fn artifact_content_for_role(role: &AgentRole, runtime_log: &str, final_message: &str) -> String {
    match role {
        AgentRole::Planner | AgentRole::Reviewer => final_message.to_string(),
        _ => runtime_log.to_string(),
    }
}

// After (fix): all roles return the lean final_message (~5KB)
fn artifact_content_for_role(_role: &AgentRole, _runtime_log: &str, final_message: &str) -> String {
    final_message.to_string()
}
```

## Why This Works

`runtime_log` is constructed by appending all streaming JSON events emitted by Codex during execution — 44× repeated boilerplate JSON per run, totalling ~2.4MB. `final_message` is extracted separately via Codex's `--output-last-message` flag into a temp file; it contains only the polished assistant response (~5KB). The original `match` was inconsistent: Planner and Reviewer already used `final_message`, but Analyst and Coder fell through to `runtime_log`. Since `runtime_log` adds no semantic signal beyond what `final_message` contains (it is a superset with noise), all roles can safely use `final_message`. The function's `role` and `runtime_log` parameters are now unused and prefixed `_`.

## Prevention

- Any new `AgentRole` variant must not use `runtime_log` as artifact content — the correct value is always `final_message`
- Add a unit test asserting that artifact content length is under 100KB for every `AgentRole` so a future regression fails fast at CI time
- `artifact_content_for_role()` is now a YAGNI pass-through (ignores both `_role` and `_runtime_log`); consider inlining it at the call site as `content: final_message.clone()` and deleting the helper to eliminate the misleading dead parameters
- Rename the local `content` variable (built from `runtime_log()`) to `failure_log` to clarify it is only used for the `CanopusError::Runtime` error path, not as artifact body

## Related Issues

- No GitHub issues on file
- Related context in `docs/v1-operator-runbook.md` (artifact vocabulary) — does not cover this bug
