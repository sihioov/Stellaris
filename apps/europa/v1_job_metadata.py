"""Canopus V1 job metadata projection for Discord-originated tasks.

These field names are Canopus-owned V1 transport metadata. Europa records them
so local Discord tasks can be inspected and routed before a shared cross-language
contract exists; mutation policy remains closed here and is executed by Canopus.
"""
import os

from instruction_router import classify_instruction


def canopus_mutation_mode_projection(github_project_mode: str | None) -> str:
    return github_project_mode or "discord-v1-live-gate-closed"


def build_v1_job_metadata(
    *,
    task_id: str,
    agenda_id: str,
    run_id: str,
    request: str,
    project: dict,
    state_root: str | None,
    github_project_mode: str | None,
) -> dict:
    classification = classify_instruction(request)
    metadata = {
        "job_id": task_id,
        "job_status": "classified",
        "intent": classification.intent,
        "classification_reason": classification.reason,
        "runner_backend": "canopus",
        "canopus_run_id": run_id,
    }
    metadata.update(_mutation_gate_metadata(github_project_mode))
    metadata.update(_branch_metadata(run_id))
    metadata.update(_worktree_metadata(project))
    metadata.update(_artifact_contract_metadata(state_root, run_id))
    return metadata


def _worktree_metadata(project: dict) -> dict:
    active_worktree = str(project.get("active_worktree") or "default")
    worktrees = project.get("worktrees") if isinstance(project.get("worktrees"), dict) else {}
    active_entry = worktrees.get(active_worktree) if isinstance(worktrees.get(active_worktree), dict) else {}
    worktree_repo_path = active_entry.get("repo_path") or project.get("repo_path")
    return {
        "worktree_readiness": "selected" if worktree_repo_path else "missing",
        "worktree_name": active_worktree,
        "worktree_repo_path": worktree_repo_path,
    }


def _mutation_gate_metadata(github_project_mode: str | None) -> dict:
    return {
        "canopus_mutation_owner": "canopus",
        "canopus_mutation_mode_projection": canopus_mutation_mode_projection(github_project_mode),
        "github_mutation_gate": "closed",
        "github_push_ready": False,
        "draft_pr_ready": False,
    }


def _branch_metadata(run_id: str) -> dict:
    return {
        "branch_readiness": "planned",
        "branch_source": "canopus_run_id",
        "planned_branch": f"canopus/{run_id}",
    }


def _artifact_contract_metadata(state_root: str | None, run_id: str) -> dict:
    metadata = {}
    if state_root:
        checkpoint_root = os.path.join(state_root, "checkpoints", run_id)
        artifact_root = os.path.join(state_root, "artifacts", run_id)
        metadata["checkpoint_root"] = checkpoint_root
        metadata["artifact_root"] = artifact_root
        metadata["artifact_paths"] = {
            "request": os.path.join(checkpoint_root, "request.json"),
            "plan_checkpoint": os.path.join(checkpoint_root, "plan-checkpoint.md"),
            "result": os.path.join(state_root, "runs", f"{run_id}.json"),
            "test_log": os.path.join(artifact_root, "test.log"),
            "diff_summary": os.path.join(artifact_root, "diff-summary.md"),
            "finalize_result": os.path.join(state_root, "runs", f"{run_id}-finalize.txt"),
            "delivery_gate": os.path.join(state_root, "runs", f"{run_id}-delivery-gate.json"),
        }
    return metadata
