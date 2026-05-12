# Canopus plugin runtime backends

Canopus now resolves a task capability before runtime execution:

- `plan` from `--role-mode planner` or `custom:canopus.planner`
- `implement` from `--role-mode agent/full/standard`, `custom:canopus.agent`, or legacy task types
- `review` from `--role-mode reviewer` or `custom:canopus.reviewer`

If `role_mode` and `task_type` imply different capabilities, `canopus submit`
fails closed before branch creation or runtime selection.

## Registry

Set `CANOPUS_BACKEND_REGISTRY_CONFIG` to a JSON file:

```json
{
  "backends": {
    "sample_a": {
      "kind": "command",
      "argv": ["/path/to/backend-a"],
      "env_allowlist": ["SAFE_HINT"]
    }
  },
  "capability_defaults": {
    "plan": "sample_a",
    "implement": "sample_a",
    "review": "sample_a"
  },
  "capability_override_allowlists": {
    "plan": ["sample_a"],
    "implement": ["sample_a"],
    "review": ["sample_a"]
  }
}
```

If no registry file is set, Canopus uses the existing legacy runtime selection
from `CANOPUS_AGENT_RUNTIME`.

## Directive

Users may select an allowed backend with an exact whitespace-delimited directive:

```text
backend=sample_a
```

Backend names must match `[A-Za-z0-9_.-]{1,64}`. Duplicate, malformed, or
disallowed directives are rejected. Fenced code blocks are ignored.

## Safety boundary

`plan` and `review` use read-only preparation and do not create a Canopus branch.
`implement` keeps the existing branch-based preparation path.

Command backends run from argv arrays, not shell strings. They start from a
cleared environment; only explicit safe allowlist variables plus Canopus task
metadata are passed. Token, secret, password, private-key, and live-mutation
environment variables are never forwarded.

Each submit run writes a machine-readable `backend-selection` artifact and run
stage record so operators can audit capability, backend, directive/default
source, and preparation policy.
