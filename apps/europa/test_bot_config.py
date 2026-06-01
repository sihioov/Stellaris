import importlib.util
import json
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path

BOT_PATH = Path(__file__).with_name("europa.py")
BOT_DIR = str(BOT_PATH.parent)


def load_bot(**env):
    for name in [
        "discord",
        "discord.ext",
        "discord.ext.commands",
        "dotenv",
        "config",
        "canopus_client",
        "instruction_router",
        "payloads",
        "projects_store",
        "tasks_store",
        "v1_job_metadata",
    ]:
        sys.modules.pop(name, None)

    discord = types.ModuleType("discord")

    class Intents:
        @classmethod
        def default(cls):
            obj = cls()
            obj.message_content = False
            obj.guilds = False
            return obj

    discord.Intents = Intents
    discord.Forbidden = type("Forbidden", (Exception,), {})
    discord.HTTPException = type("HTTPException", (Exception,), {})
    discord.utils = types.SimpleNamespace(get=lambda items, **kwargs: None)

    ext = types.ModuleType("discord.ext")
    commands = types.ModuleType("discord.ext.commands")

    class Bot:
        def __init__(self, *args, **kwargs):
            self.user = types.SimpleNamespace(id="bot")

        def command(self, *args, **kwargs):
            return lambda fn: fn

        def event(self, fn):
            return fn

        def run(self, token):  # pragma: no cover - import guard prevents this
            raise AssertionError("bot.run should not be called in tests")

    commands.Bot = Bot
    ext.commands = commands
    discord.ext = ext
    sys.modules["discord"] = discord
    sys.modules["discord.ext"] = ext
    sys.modules["discord.ext.commands"] = commands

    dotenv = types.ModuleType("dotenv")
    dotenv.load_dotenv = lambda: None
    sys.modules["dotenv"] = dotenv

    old_env = os.environ.copy()
    old_path = sys.path.copy()
    os.environ.clear()
    os.environ.update(env)
    try:
        sys.path.insert(0, BOT_DIR)
        spec = importlib.util.spec_from_file_location("discord_bot_under_test", BOT_PATH)
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        return module
    finally:
        os.environ.clear()
        os.environ.update(old_env)
        sys.path[:] = old_path


class FakeCtx:
    def __init__(self, category_id=10):
        self.sent = []
        self.guild = types.SimpleNamespace(id=1)
        self.channel = types.SimpleNamespace(
            id=2,
            name="development",
            category=types.SimpleNamespace(id=category_id),
        )
        self.message = types.SimpleNamespace(id=3)
        self.author = types.SimpleNamespace(id=4)

    async def send(self, message):
        self.sent.append(message)

    def typing(self):
        return AsyncNoopTyping()


class AsyncNoopTyping:
    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc, tb):
        return False


def configure_project(bot, tmp, task):
    tasks_path = Path(tmp) / "tasks.json"
    projects_path = Path(tmp) / "projects.json"
    bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
    bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
    bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})
    tasks_path.write_text(json.dumps([task]), encoding="utf-8")
    return tasks_path


class DiscordBotConfigTests(unittest.TestCase):
    def test_instruction_router_classifies_read_only_analysis(self):
        bot = load_bot()

        classification = bot.classify_instruction("look at the auth flow and explain the issue")

        self.assertEqual(classification.intent, bot.READ_ONLY_ANALYSIS)
        self.assertEqual(classification.reason, "read_only_analysis")

    def test_instruction_router_classifies_code_change(self):
        bot = load_bot()

        classification = bot.classify_instruction("implement the retry button in europa")

        self.assertEqual(classification.intent, bot.CODE_CHANGE)
        self.assertEqual(classification.reason, "explicit_code_change")

    def test_instruction_router_classifies_pr_review(self):
        bot = load_bot()

        classification = bot.classify_instruction("review PR #42 before merge")

        self.assertEqual(classification.intent, bot.PR_REVIEW)
        self.assertEqual(classification.reason, "pr_review_reference")

    def test_instruction_router_classifies_ci_repair(self):
        bot = load_bot()

        classification = bot.classify_instruction("CI failed on the lint job, fix it")

        self.assertEqual(classification.intent, bot.CI_REPAIR)
        self.assertEqual(classification.reason, "ci_failure_repair")

    def test_instruction_router_keeps_ambiguous_requests_non_mutating(self):
        bot = load_bot()

        classification = bot.classify_instruction("fix it")

        self.assertEqual(classification.intent, bot.NEEDS_CLARIFICATION)
        self.assertEqual(classification.reason, "objectless_command")

    def test_instruction_router_requires_clarification_for_unknown_intent(self):
        bot = load_bot()

        classification = bot.classify_instruction("")

        self.assertEqual(classification.intent, bot.NEEDS_CLARIFICATION)
        self.assertEqual(classification.reason, "empty_request")

    def test_tasks_json_path_overrides_per_category_file(self):
        bot = load_bot(TASKS_JSON_PATH="/tmp/stellaris-tasks.json")
        self.assertEqual(bot.get_tasks_path(123), "/tmp/stellaris-tasks.json")

    def test_channel_type_map_includes_conversation_channels(self):
        bot = load_bot()

        self.assertIsNone(bot.CHANNEL_TYPE_MAP["analysis"])
        self.assertIsNone(bot.CHANNEL_TYPE_MAP["brainstorming"])
        self.assertIsNone(bot.get_channel_type("analysis"))
        self.assertIsNone(bot.get_channel_type("brainstorming"))
        self.assertEqual(bot.get_channel_type("planning"), "canopus.planner")
        self.assertEqual(bot.get_channel_type("development"), "canopus.agent")
        self.assertEqual(bot.get_channel_type("review"), "canopus.reviewer")

    def test_git_repo_path_accepts_linked_worktree_git_file(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, ".git").write_text("gitdir: /tmp/example", encoding="utf-8")

            self.assertTrue(bot.is_git_repo_path(tmp))

    def test_task_payload_includes_agenda_and_github_metadata(self):
        bot = load_bot(
            GITHUB_OWNER="acme",
            GITHUB_REPO="demo",
            GITHUB_PROJECT_ID="PVT_kwDOdemo",
            GITHUB_PROJECT_URL="https://github.com/orgs/acme/projects/1",
            GITHUB_PROJECT_OWNER_KIND="org",
            GITHUB_PROJECT_OWNER="acme",
            GITHUB_PROJECT_NUMBER="1",
            GITHUB_PROJECT_STATUS_FIELD_NAME="Status",
            GITHUB_PROJECT_STATUS_OPTION_NAME="Ready",
            CANOPUS_GITHUB_PROJECT_MODE="dry-run-offline",
        )
        ctx = types.SimpleNamespace(
            guild=types.SimpleNamespace(id=1),
            channel=types.SimpleNamespace(id=2),
            message=types.SimpleNamespace(id=3),
        )

        payload = bot.build_task_payload(
            ctx,
            "discord-abc123",
            "Ship agenda metadata",
            {"repo_path": "/repo"},
            "canopus.reviewer",
        )

        self.assertEqual(payload["agenda_id"], "agenda-discord-abc123")
        self.assertEqual(payload["role_mode"], "review")
        self.assertEqual(payload["github_repo_slug"], "acme/demo")
        self.assertIn("https://github.com/acme/demo/issues/new", payload["github_issue_create_url"])
        self.assertEqual(payload["github_project_id"], "PVT_kwDOdemo")
        self.assertEqual(payload["github_project_owner_kind"], "org")
        self.assertEqual(payload["github_project_owner"], "acme")
        self.assertEqual(payload["github_project_number"], "1")
        self.assertEqual(payload["github_project_status_field_name"], "Status")
        self.assertEqual(payload["github_project_status_option_name"], "Ready")
        self.assertEqual(payload["github_project_mode"], "dry-run-offline")
        self.assertEqual(payload["discord_message_url"], "https://discord.com/channels/1/2/3")
        self.assertEqual(payload["discord_parent_channel_id"], "2")
        self.assertEqual(payload["discord_context_kind"], "message")
        self.assertEqual(payload["discord_context_id"], "discord-message-1-2-3")
        self.assertEqual(payload["follow_up_source"], "discord")
        self.assertEqual(payload["follow_up_channel_id"], "2")
        self.assertEqual(payload["follow_up_message_id"], "3")
        self.assertEqual(payload["follow_up_message_url"], "https://discord.com/channels/1/2/3")
        self.assertEqual(payload["confirmation_state"], "requested")
        self.assertEqual(payload["job_id"], "discord-abc123")
        self.assertEqual(payload["job_status"], "classified")
        self.assertEqual(payload["intent"], bot.NEEDS_CLARIFICATION)
        self.assertEqual(payload["classification_reason"], "no_stable_intent_match")
        self.assertEqual(payload["runner_backend"], "canopus")
        self.assertEqual(payload["canopus_run_id"], "agenda-discord-abc123-discord-abc123")
        self.assertEqual(payload["planned_branch"], "canopus/agenda-discord-abc123-discord-abc123")
        self.assertEqual(payload["branch_source"], "canopus_run_id")
        self.assertEqual(payload["branch_readiness"], "planned")
        self.assertEqual(payload["canopus_mutation_owner"], "canopus")
        self.assertEqual(payload["canopus_mutation_mode_projection"], "dry-run-offline")
        self.assertEqual(payload["github_mutation_gate"], "closed")
        self.assertFalse(payload["github_push_ready"])
        self.assertFalse(payload["draft_pr_ready"])
        self.assertEqual(payload["worktree_readiness"], "selected")
        self.assertEqual(payload["worktree_name"], "default")
        self.assertEqual(payload["worktree_repo_path"], "/repo")
        self.assertEqual(payload["checkpoint_root"], "/repo/.canopus/checkpoints/agenda-discord-abc123-discord-abc123")
        self.assertEqual(payload["artifact_root"], "/repo/.canopus/artifacts/agenda-discord-abc123-discord-abc123")
        self.assertEqual(
            payload["artifact_paths"],
            {
                "request": "/repo/.canopus/checkpoints/agenda-discord-abc123-discord-abc123/request.json",
                "plan_checkpoint": "/repo/.canopus/checkpoints/agenda-discord-abc123-discord-abc123/plan-checkpoint.md",
                "result": "/repo/.canopus/runs/agenda-discord-abc123-discord-abc123.json",
                "test_log": "/repo/.canopus/artifacts/agenda-discord-abc123-discord-abc123/test.log",
                "diff_summary": "/repo/.canopus/artifacts/agenda-discord-abc123-discord-abc123/diff-summary.md",
                "finalize_result": "/repo/.canopus/runs/agenda-discord-abc123-discord-abc123-finalize.txt",
                "delivery_gate": "/repo/.canopus/runs/agenda-discord-abc123-discord-abc123-delivery-gate.json",
            },
        )

    def test_task_payload_records_real_discord_thread_context_when_available(self):
        bot = load_bot()
        ctx = types.SimpleNamespace(
            guild=types.SimpleNamespace(id=1),
            channel=types.SimpleNamespace(id=20, parent_id=2),
            message=types.SimpleNamespace(id=3),
            author=types.SimpleNamespace(id=4),
        )

        payload = bot.build_task_payload(
            ctx,
            "discord-abc123",
            "review PR #42",
            {"repo_path": "/repo"},
            "canopus.reviewer",
        )

        self.assertEqual(payload["discord_thread_id"], "20")
        self.assertEqual(payload["discord_parent_channel_id"], "2")
        self.assertEqual(payload["discord_context_kind"], "thread")
        self.assertEqual(payload["discord_context_id"], "discord-thread-20")
        self.assertEqual(payload["follow_up_user_id"], "4")
        self.assertEqual(payload["intent"], bot.PR_REVIEW)

    def test_artifact_paths_include_payload_contract_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / ".canopus"
            bot = load_bot(CANOPUS_STATE_PATH=str(state))
            result_path = state / "runs" / "run-1.json"
            outside_path = Path(tmp) / "outside.json"
            result_path.parent.mkdir(parents=True)
            result_path.write_text("{}", encoding="utf-8")
            outside_path.write_text("{}", encoding="utf-8")
            task = {
                "task_id": "discord-1",
                "payload": json.dumps(
                    {
                        "artifact_paths": {
                            "result": str(result_path),
                            "outside": str(outside_path),
                        }
                    }
                ),
            }

            paths = bot._artifact_paths(None, task)

            self.assertIn(str(result_path), paths)
            self.assertNotIn(str(outside_path), paths)

    def test_artifact_lookup_ids_cannot_escape_state_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / ".canopus"
            outside = Path(tmp) / "outside-dir"
            outside.mkdir()
            (outside / "leak.txt").write_text("secret", encoding="utf-8")
            bot = load_bot(CANOPUS_STATE_PATH=str(state))
            task = {
                "task_id": "discord-1",
                "payload": json.dumps({"run_id": "../../outside-dir"}),
            }

            paths = bot._artifact_paths(None, task)

            self.assertNotIn(str(outside / "leak.txt"), paths)

    def test_artifact_lookup_rejects_symlink_escape_from_state_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / ".canopus"
            outside = Path(tmp) / "outside-dir"
            outside.mkdir()
            (outside / "leak.txt").write_text("secret", encoding="utf-8")
            artifact_root = state / "artifacts"
            artifact_root.mkdir(parents=True)
            (artifact_root / "run-1").symlink_to(outside, target_is_directory=True)
            bot = load_bot(CANOPUS_STATE_PATH=str(state))
            task = {
                "task_id": "discord-1",
                "payload": json.dumps({"run_id": "run-1"}),
            }

            paths = bot._artifact_paths(None, task)

            self.assertNotIn(str(outside / "leak.txt"), paths)
            self.assertEqual(paths, [])

    def test_task_payload_omits_mutating_project_mode_from_discord_metadata(self):
        bot = load_bot(
            GITHUB_OWNER="acme",
            GITHUB_REPO="demo",
            GITHUB_PROJECT_ID="PVT_kwDOdemo",
            CANOPUS_GITHUB_PROJECT_MODE="mutate-live",
        )
        ctx = types.SimpleNamespace(
            guild=types.SimpleNamespace(id=1),
            channel=types.SimpleNamespace(id=2),
            message=types.SimpleNamespace(id=3),
        )

        payload = bot.build_task_payload(
            ctx,
            "discord-abc123",
            "Ship agenda metadata",
            {"repo_path": "/repo"},
            "canopus.agent",
        )

        self.assertNotIn("github_project_mode", payload)
        self.assertEqual(payload["canopus_mutation_mode_projection"], "discord-v1-live-gate-closed")
        self.assertEqual(payload["github_mutation_gate"], "closed")
        self.assertFalse(payload["github_push_ready"])
        self.assertFalse(payload["draft_pr_ready"])
        self.assertIsNone(payload.get("github_issue_number"))
        self.assertIsNone(payload.get("github_issue_url"))

    def test_task_payload_does_not_carry_runtime_backend_selection(self):
        bot = load_bot()
        ctx = types.SimpleNamespace(
            guild=types.SimpleNamespace(id=1),
            channel=types.SimpleNamespace(id=2),
            message=types.SimpleNamespace(id=3),
        )

        payload = bot.build_task_payload(
            ctx,
            "discord-abc123",
            "backend=sample_b remains request text only",
            {"repo_path": "/repo"},
            "canopus.planner",
        )

        self.assertEqual(payload["role_mode"], "plan")
        self.assertEqual(payload["request"], "backend=sample_b remains request text only")
        self.assertNotIn("backend", payload)
        self.assertNotIn("runtime_backend", payload)
        self.assertNotIn("canopus_backend", payload)

    def test_task_payload_uses_deterministic_agenda_id_when_work_intake_carries_issue(self):
        bot = load_bot(GITHUB_OWNER="acme", GITHUB_REPO="demo")
        ctx = types.SimpleNamespace(
            guild=types.SimpleNamespace(id=1),
            channel=types.SimpleNamespace(id=2),
            message=types.SimpleNamespace(id=3),
        )
        intake = {
            "github_owner": "Acme",
            "github_repo": "Demo",
            "github_issue_number": 42,
            "github_mutation_gate": "open",
            "github_push_ready": True,
            "draft_pr_ready": True,
        }

        payload = bot.build_task_payload(
            ctx,
            "discord-abc123",
            "Ship agenda metadata",
            {"repo_path": "/repo"},
            "canopus.agent",
            intake,
        )

        # gh-{owner}-{repo}-{number} sanitised by run-identity rules (lowercase, dash-collapsed)
        self.assertEqual(payload["agenda_id"], "gh-acme-demo-42")
        self.assertEqual(payload["canopus_agenda_id"], "gh-acme-demo-42")
        self.assertEqual(payload["github_mutation_gate"], "closed")
        self.assertFalse(payload["github_push_ready"])
        self.assertFalse(payload["draft_pr_ready"])

    def test_task_payload_keeps_legacy_agenda_id_without_issue_identity(self):
        bot = load_bot()
        ctx = types.SimpleNamespace(
            guild=types.SimpleNamespace(id=1),
            channel=types.SimpleNamespace(id=2),
            message=types.SimpleNamespace(id=3),
        )

        payload = bot.build_task_payload(
            ctx,
            "discord-zzz",
            "no github context",
            {"repo_path": "/repo"},
            "canopus.agent",
        )

        # No work_intake, no env owner/repo => existing agenda-{task_id} form preserved.
        self.assertEqual(payload["agenda_id"], "agenda-discord-zzz")

    def test_resolve_agenda_id_helper_priority(self):
        bot = load_bot(GITHUB_OWNER="acme", GITHUB_REPO="demo")
        # Priority 1: explicit issue identity in any source dict.
        self.assertEqual(
            bot.resolve_agenda_id(
                "discord-1",
                {"github_owner": "Acme", "github_repo": "Demo", "github_issue_number": 7},
            ),
            "gh-acme-demo-7",
        )
        # Priority 2: env owner/repo with number from a source dict.
        self.assertEqual(
            bot.resolve_agenda_id(
                "discord-1",
                {"github_issue_number": "8"},  # string number must be coerced
            ),
            "gh-acme-demo-8",
        )
        # No issue number anywhere => task-id-based fallback.
        self.assertEqual(
            bot.resolve_agenda_id("discord-1", {"github_owner": "acme", "github_repo": "demo"}),
            "agenda-discord-1",
        )

    def test_deterministic_agenda_id_helper_matches_canopus_run_identity_rules(self):
        bot = load_bot()
        # Same identity twice => same id.
        a = bot.deterministic_agenda_id_for_github_issue("acme", "demo", 42)
        b = bot.deterministic_agenda_id_for_github_issue("acme", "demo", 42)
        self.assertEqual(a, b)
        self.assertEqual(a, "gh-acme-demo-42")
        # Sanitises uppercase / spaces / slashes the same way Rust does.
        self.assertEqual(
            bot.deterministic_agenda_id_for_github_issue("Acme/Org", "Demo Repo", 9001),
            "gh-acme-org-demo-repo-9001",
        )
        # Different identity => different id.
        self.assertNotEqual(
            bot.deterministic_agenda_id_for_github_issue("acme", "demo", 1),
            bot.deterministic_agenda_id_for_github_issue("acme", "demo", 2),
        )
        # Non-ASCII owner/repo must still produce a usable id thanks to the
        # ``gh-`` prefix surviving sanitisation (mirrors Rust derive_run_identity).
        non_ascii = bot.deterministic_agenda_id_for_github_issue("…", "—", 5)
        self.assertTrue(non_ascii.startswith("gh"))
        self.assertIn("5", non_ascii)

    def test_approval_hook_updates_payload_and_finalize_signal(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            tasks_path = Path(tmp) / "tasks.json"
            task = {
                "task_id": "discord-1",
                "task_type": {"Custom": "canopus.agent"},
                "payload": json.dumps(
                    {
                        "agenda_id": "agenda-discord-1",
                        "job_id": "discord-1",
                        "intent": "CodeChange",
                        "discord_context_id": "discord-message-1-2-3",
                        "artifact_paths": {"result": "/repo/.canopus/runs/run.json"},
                        "approval_state": "pending",
                    }
                ),
                "meta": {"status": "PendingReview"},
            }
            tasks_path.write_text(json.dumps([task]), encoding="utf-8")

            target, error = bot.update_task_status_locked(
                str(tasks_path),
                "discord-1",
                "approve",
                {"PendingReview"},
                "PendingReview",
                "Processed",
                bot.mark_task_approved,
            )

            self.assertIsNone(error)
            self.assertEqual(target["meta"]["status"], "Processed")
            payload = json.loads(target["payload"])
            self.assertEqual(payload["approval_state"], "approved")
            self.assertEqual(payload["confirmation_state"], "approved")
            self.assertIsNotNone(payload["finalize_requested_at"])
            self.assertEqual(payload.get("job_id"), "discord-1")
            self.assertEqual(payload.get("intent"), "CodeChange")
            self.assertEqual(payload.get("discord_context_id"), "discord-message-1-2-3")
            self.assertEqual(payload.get("artifact_paths", {}).get("result"), "/repo/.canopus/runs/run.json")
            stored = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            self.assertEqual(stored["meta"]["approval_state"], "approved")

    def test_approval_and_rejection_preserve_v1_job_context(self):
        bot = load_bot()
        base_payload = {
            "agenda_id": "agenda-discord-1",
            "job_id": "discord-1",
            "intent": "CodeChange",
            "classification_reason": "explicit_code_change",
            "discord_context_id": "discord-message-1-2-3",
            "artifact_paths": {"result": "/repo/.canopus/runs/run.json"},
            "approval_state": "pending",
        }
        approved = {"task_id": "discord-1", "payload": json.dumps(base_payload), "meta": {}}
        rejected = {"task_id": "discord-1", "payload": json.dumps(base_payload), "meta": {}}

        bot.mark_task_approved(approved)
        bot.mark_task_rejected(rejected)

        approved_payload = json.loads(approved["payload"])
        rejected_payload = json.loads(rejected["payload"])
        for payload in (approved_payload, rejected_payload):
            self.assertEqual(payload["job_id"], "discord-1")
            self.assertEqual(payload["intent"], "CodeChange")
            self.assertEqual(payload["classification_reason"], "explicit_code_change")
            self.assertEqual(payload["discord_context_id"], "discord-message-1-2-3")
            self.assertEqual(payload["artifact_paths"]["result"], "/repo/.canopus/runs/run.json")
        self.assertEqual(approved_payload["approval_state"], "approved")
        self.assertEqual(rejected_payload["approval_state"], "rejected")

    def test_approval_hook_records_discord_provenance_when_provided(self):
        bot = load_bot()
        task = {
            "task_id": "discord-1",
            "task_type": {"Custom": "canopus.agent"},
            "payload": json.dumps({"agenda_id": "agenda-discord-1", "approval_state": "pending"}),
            "meta": {"status": "PendingReview"},
        }

        bot.mark_task_approved(
            task,
            approved_by="1234",
            approval_source="discord",
            approval_message_url="https://discord.com/channels/1/2/3",
        )

        payload = json.loads(task["payload"])
        self.assertEqual(payload["approved_by"], "1234")
        self.assertEqual(payload["approval_source"], "discord")
        self.assertEqual(payload["approval_message_url"], "https://discord.com/channels/1/2/3")
        self.assertEqual(task["meta"]["approved_by"], "1234")
        self.assertEqual(task["meta"]["approval_source"], "discord")

    def test_promote_pending_proposal_with_intake_records_success_metadata(self):
        bot = load_bot()
        task = {
            "task_id": "discord-proposal",
            "payload": json.dumps({"request": "candidate"}),
            "meta": {"status": "PendingProposal"},
        }
        intake = {
            "github_issue_number": 42,
            "github_issue_url": "https://github.test/acme/demo/issues/42",
            "github_project_item_id": "PVTI_1",
            "github_mutation_gate": "open",
            "github_push_ready": True,
            "draft_pr_ready": True,
            "ignored": "not copied",
        }

        bot.promote_pending_proposal_with_intake(task, intake)

        payload = json.loads(task["payload"])
        self.assertEqual(payload["proposal_intake_state"], "succeeded")
        self.assertIsNotNone(payload["proposal_intake_attempted_at"])
        self.assertEqual(payload["github_issue_number"], 42)
        self.assertEqual(payload["github_project_item_id"], "PVTI_1")
        self.assertNotIn("github_mutation_gate", payload)
        self.assertNotIn("github_push_ready", payload)
        self.assertNotIn("draft_pr_ready", payload)
        self.assertNotIn("ignored", payload)
        self.assertEqual(task["meta"]["proposal_intake_state"], "succeeded")

    def test_promote_pending_proposal_without_intake_marks_not_required(self):
        bot = load_bot()
        task = {
            "task_id": "local-proposal",
            "payload": json.dumps({"request": "candidate"}),
            "meta": {"status": "PendingProposal"},
        }

        bot.promote_pending_proposal_with_intake(task, None)

        payload = json.loads(task["payload"])
        self.assertNotIn("proposal_intake_state", payload)
        self.assertEqual(task["meta"]["proposal_intake_state"], "not_required")

    def test_task_payload_carries_repo_path_for_state_routing(self):
        """repo_path must be present in payload so Laniakea/canopus-watch can
        derive the per-project .canopus/ state root (PR-A payload-driven routing)."""
        bot = load_bot()
        ctx = types.SimpleNamespace(
            guild=types.SimpleNamespace(id=1),
            channel=types.SimpleNamespace(id=2),
            message=types.SimpleNamespace(id=3),
        )

        payload = bot.build_task_payload(
            ctx,
            "discord-abc123",
            "multi-project routing smoke",
            {"repo_path": "/home/user/project/MyNewProject"},
            "canopus.agent",
        )

        self.assertEqual(payload["repo_path"], "/home/user/project/MyNewProject")

    def test_default_new_project_path_uses_configured_project_root(self):
        bot = load_bot(NEW_PROJECT_DEFAULT_ROOT="/tmp/europa-projects")

        self.assertEqual(
            bot.default_new_project_repo_path("demo"),
            "/tmp/europa-projects/demo",
        )

    def test_default_new_project_path_rejects_path_escape_names(self):
        bot = load_bot(NEW_PROJECT_DEFAULT_ROOT="/tmp/europa-projects")

        with self.assertRaises(ValueError):
            bot.default_new_project_repo_path("../demo")

class DiscordBotGitHubBoundaryTests(unittest.IsolatedAsyncioTestCase):
    async def test_new_project_without_path_creates_under_default_project_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            bot = load_bot(NEW_PROJECT_DEFAULT_ROOT=str(Path(tmp) / "projects"))
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.discord.utils.get = lambda items, **kwargs: None

            class FakeCategory:
                def __init__(self, name, category_id=123):
                    self.name = name
                    self.id = category_id
                    self.channels = []

                async def create_text_channel(self, name):
                    self.channels.append(name)

            class FakeGuild:
                def __init__(self):
                    self.categories = []
                    self.created_categories = []

                async def create_category(self, name):
                    category = FakeCategory(name)
                    self.created_categories.append(category)
                    return category

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = FakeGuild()
                    self.channel = types.SimpleNamespace(id=2, name="general", category=None)
                    self.message = types.SimpleNamespace(id=3)
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_new_project(ctx, name="demo")

            expected_repo = Path(tmp) / "projects" / "demo"
            self.assertTrue((expected_repo / ".git").exists())
            stored = bot.read_projects()["projects"]["123"]
            self.assertEqual(stored["repo_path"], str(expected_repo))
            self.assertEqual(
                ctx.guild.created_categories[0].channels,
                ["general", "planning", "development", "review", "analysis", "brainstorming"],
            )
            self.assertIn("프로젝트 생성 완료", ctx.sent[0])
            self.assertIn("6채널", ctx.sent[0])

    async def test_finalize_approved_task_uses_bounded_json_command(self):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / "fake_canopus.py"
            argv_path = Path(tmp) / "argv.json"
            script.write_text(
                "import json, pathlib, sys\n"
                f"pathlib.Path({str(argv_path)!r}).write_text(json.dumps(sys.argv[1:]), encoding='utf-8')\n"
                "print(json.dumps({'ok': True, 'status': 'dry_run', 'task_id': 'discord-1'}))\n",
                encoding="utf-8",
            )
            bot = load_bot(CANOPUS_COMMAND=f"{sys.executable} {script}")

            result, error = await bot.finalize_approved_task("/tmp/tasks.json", "discord-1")

            self.assertIsNone(error)
            self.assertEqual(result["status"], "dry_run")
            self.assertEqual(
                json.loads(argv_path.read_text(encoding="utf-8")),
                [
                    "finalize-approved",
                    "--tasks",
                    "/tmp/tasks.json",
                    "--task-id",
                    "discord-1",
                    "--json",
                ],
            )

    async def test_run_ask_backend_accepts_contextual_cwd_env_and_command_label(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            result_path = Path(tmp) / "ask-result.json"
            script = Path(tmp) / "fake_ask.py"
            script.write_text(
                "import json, os, pathlib, sys\n"
                "payload = {\n"
                "  'stdin': sys.stdin.read(),\n"
                "  'cwd': os.getcwd(),\n"
                "  'prompt': os.environ.get('STELLARIS_ASK_PROMPT'),\n"
                "  'project': os.environ.get('STELLARIS_PROJECT_NAME'),\n"
                "  'repo': os.environ.get('STELLARIS_PROJECT_REPO_PATH'),\n"
                "  'channel': os.environ.get('STELLARIS_DISCORD_CHANNEL'),\n"
                "  'role': os.environ.get('STELLARIS_CONVERSATION_ROLE'),\n"
                "}\n"
                f"pathlib.Path({str(result_path)!r}).write_text(json.dumps(payload), encoding='utf-8')\n"
                "print('ok')\n",
                encoding="utf-8",
            )
            bot = load_bot(ASK_COMMAND=f"{sys.executable} {script}")

            answer, error = await bot.run_ask_backend(
                "contextual prompt",
                cwd=str(repo),
                extra_env={
                    "STELLARIS_PROJECT_NAME": "Demo",
                    "STELLARIS_PROJECT_REPO_PATH": str(repo),
                    "STELLARIS_DISCORD_CHANNEL": "#analysis",
                    "STELLARIS_CONVERSATION_ROLE": "analysis",
                },
                command_label="!analyze",
            )

            self.assertIsNone(error)
            self.assertEqual(answer, "ok")
            payload = json.loads(result_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["stdin"], "contextual prompt")
            self.assertEqual(payload["cwd"], str(repo))
            self.assertEqual(payload["prompt"], "contextual prompt")
            self.assertEqual(payload["project"], "Demo")
            self.assertEqual(payload["repo"], str(repo))
            self.assertEqual(payload["channel"], "#analysis")
            self.assertEqual(payload["role"], "analysis")

    async def test_run_ask_backend_uses_command_label_in_configuration_errors(self):
        bot = load_bot(ASK_COMMAND="")

        _answer, error = await bot.run_ask_backend("prompt", command_label="!analyze")

        self.assertIn("`!analyze`", error)

    async def test_run_ask_backend_preserves_plain_defaults(self):
        with tempfile.TemporaryDirectory() as tmp:
            result_path = Path(tmp) / "plain-ask.json"
            script = Path(tmp) / "fake_ask.py"
            script.write_text(
                "import json, os, pathlib, sys\n"
                "payload = {\n"
                "  'stdin': sys.stdin.read(),\n"
                "  'prompt': os.environ.get('STELLARIS_ASK_PROMPT'),\n"
                "  'project': os.environ.get('STELLARIS_PROJECT_NAME'),\n"
                "  'role': os.environ.get('STELLARIS_CONVERSATION_ROLE'),\n"
                "}\n"
                f"pathlib.Path({str(result_path)!r}).write_text(json.dumps(payload), encoding='utf-8')\n"
                "print('plain ok')\n",
                encoding="utf-8",
            )
            bot = load_bot(ASK_COMMAND=f"{sys.executable} {script}")

            answer, error = await bot.run_ask_backend("plain question")

            self.assertIsNone(error)
            self.assertEqual(answer, "plain ok")
            payload = json.loads(result_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["stdin"], "plain question")
            self.assertEqual(payload["prompt"], "plain question")
            self.assertIsNone(payload["project"])
            self.assertIsNone(payload["role"])

    async def test_run_canopus_json_preserves_partial_failure_stdout_object(self):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / "fake_canopus.py"
            script.write_text(
                "import json, sys\n"
                "print(json.dumps({'ok': False, 'error': 'project sync failed', 'github_issue_number': 42}))\n"
                "print('stderr detail', file=sys.stderr)\n"
                "sys.exit(1)\n",
                encoding="utf-8",
            )
            bot = load_bot(CANOPUS_COMMAND=f"{sys.executable} {script}")

            result, error = await bot.run_canopus_json(["work-intake"])

            self.assertEqual(result["ok"], False)
            self.assertEqual(result["github_issue_number"], 42)
            self.assertIn("stderr detail", error)

    async def test_run_canopus_json_parses_ok_false_finalize_stdout_on_nonzero_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / "fake_canopus.py"
            script.write_text(
                "import json, sys\n"
                "print(json.dumps({'ok': False, 'status': 'failed', 'task_id': 'discord-1', 'retryable': True, 'error': {'code': 'git_commit_failed', 'message': 'commit failed', 'retryable': True, 'details': {'stderr': 'boom'}}}))\n"
                "sys.exit(2)\n",
                encoding="utf-8",
            )
            bot = load_bot(CANOPUS_COMMAND=f"{sys.executable} {script}")

            result, error = await bot.finalize_approved_task("/tmp/tasks.json", "discord-1")

            self.assertEqual(result["ok"], False)
            self.assertEqual(result["error"]["code"], "git_commit_failed")
            self.assertIn("commit failed", str(error))

    async def test_analyze_wrong_channel_does_not_call_backend_or_mutation_paths(self):
        bot = load_bot()
        calls = []

        async def fake_backend(*args, **kwargs):
            calls.append(("backend", args, kwargs))
            return "should not happen", None

        async def fake_intake(*args, **kwargs):
            calls.append(("intake", args, kwargs))
            return None, None

        def fake_append(*args, **kwargs):
            calls.append(("append", args, kwargs))

        bot.run_ask_backend = fake_backend
        bot.intake_github_work = fake_intake
        bot.append_task_locked = fake_append
        ctx = FakeCtx()
        ctx.channel.name = "brainstorming"

        await bot.cmd_analyze(ctx, topic="inspect")

        self.assertTrue(any("#analysis" in msg for msg in ctx.sent))
        self.assertEqual(calls, [])

    async def test_brainstorm_wrong_channel_does_not_call_backend_or_mutation_paths(self):
        bot = load_bot()
        calls = []

        async def fake_backend(*args, **kwargs):
            calls.append(("backend", args, kwargs))
            return "should not happen", None

        async def fake_intake(*args, **kwargs):
            calls.append(("intake", args, kwargs))
            return None, None

        def fake_append(*args, **kwargs):
            calls.append(("append", args, kwargs))

        bot.run_ask_backend = fake_backend
        bot.intake_github_work = fake_intake
        bot.append_task_locked = fake_append
        ctx = FakeCtx()
        ctx.channel.name = "analysis"

        await bot.cmd_brainstorm(ctx, topic="ideas")

        self.assertTrue(any("#brainstorming" in msg for msg in ctx.sent))
        self.assertEqual(calls, [])

    async def test_analyze_requires_registered_project(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            bot._projects_store.PROJECTS_JSON_PATH = str(Path(tmp) / "projects.json")
            bot._projects_store.TASKS_JSON_PATH = str(Path(tmp) / "tasks.json")
            bot.write_projects({"projects": {}})
            calls = []

            async def fake_backend(*args, **kwargs):
                calls.append(("backend", args, kwargs))
                return "should not happen", None

            bot.run_ask_backend = fake_backend
            ctx = FakeCtx()
            ctx.channel.name = "analysis"

            await bot.cmd_analyze(ctx, topic="inspect")

            self.assertTrue(any("등록된 프로젝트" in msg for msg in ctx.sent))
            self.assertEqual(calls, [])

    async def test_brainstorm_requires_registered_project(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            bot._projects_store.PROJECTS_JSON_PATH = str(Path(tmp) / "projects.json")
            bot._projects_store.TASKS_JSON_PATH = str(Path(tmp) / "tasks.json")
            bot.write_projects({"projects": {}})
            calls = []

            async def fake_backend(*args, **kwargs):
                calls.append(("backend", args, kwargs))
                return "should not happen", None

            bot.run_ask_backend = fake_backend
            ctx = FakeCtx()
            ctx.channel.name = "brainstorming"

            await bot.cmd_brainstorm(ctx, topic="ideas")

            self.assertTrue(any("등록된 프로젝트" in msg for msg in ctx.sent))
            self.assertEqual(calls, [])

    async def test_analyze_calls_backend_with_project_context(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": str(repo)}}})
            calls = []

            async def fake_backend(question, **kwargs):
                calls.append((question, kwargs))
                return "analysis ok", None

            async def fail_intake(*args, **kwargs):
                raise AssertionError("analysis must not call GitHub intake")

            def fail_append(*args, **kwargs):
                raise AssertionError("analysis must not append tasks")

            bot.run_ask_backend = fake_backend
            bot.intake_github_work = fail_intake
            bot.append_task_locked = fail_append
            ctx = FakeCtx()
            ctx.channel.name = "analysis"

            await bot.cmd_analyze(ctx, topic="inspect repo")

            self.assertEqual(len(calls), 1)
            question, kwargs = calls[0]
            self.assertIn("Project context:", question)
            self.assertIn("demo", question)
            self.assertIn(str(repo), question)
            self.assertIn("#analysis", question)
            self.assertIn("analyst-style", question)
            self.assertIn(bot.ANALYST_STYLE_INSTRUCTION, question)
            self.assertIn("inspect repo", question)
            self.assertEqual(kwargs["cwd"], str(repo))
            self.assertEqual(kwargs["command_label"], "!analyze")
            self.assertEqual(kwargs["extra_env"]["STELLARIS_PROJECT_NAME"], "demo")
            self.assertEqual(kwargs["extra_env"]["STELLARIS_PROJECT_REPO_PATH"], str(repo))
            self.assertEqual(kwargs["extra_env"]["STELLARIS_DISCORD_CHANNEL"], "#analysis")
            self.assertEqual(kwargs["extra_env"]["STELLARIS_CONVERSATION_ROLE"], "analysis")
            self.assertTrue(any("Analyze" in msg and "analysis ok" in msg for msg in ctx.sent))
            self.assertFalse(tasks_path.exists())

    async def test_brainstorm_calls_backend_with_project_context(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": str(repo)}}})
            calls = []

            async def fake_backend(question, **kwargs):
                calls.append((question, kwargs))
                return "brainstorm ok", None

            async def fail_intake(*args, **kwargs):
                raise AssertionError("brainstorm must not call GitHub intake")

            def fail_append(*args, **kwargs):
                raise AssertionError("brainstorm must not append tasks")

            bot.run_ask_backend = fake_backend
            bot.intake_github_work = fail_intake
            bot.append_task_locked = fail_append
            ctx = FakeCtx()
            ctx.channel.name = "brainstorming"

            await bot.cmd_brainstorm(ctx, topic="new directions")

            self.assertEqual(len(calls), 1)
            question, kwargs = calls[0]
            self.assertIn("Project context:", question)
            self.assertIn("demo", question)
            self.assertIn(str(repo), question)
            self.assertIn("#brainstorming", question)
            self.assertIn("brainstormer-style", question)
            self.assertIn(bot.BRAINSTORMER_STYLE_INSTRUCTION, question)
            self.assertIn("new directions", question)
            self.assertEqual(kwargs["cwd"], str(repo))
            self.assertEqual(kwargs["command_label"], "!brainstorm")
            self.assertEqual(kwargs["extra_env"]["STELLARIS_CONVERSATION_ROLE"], "brainstorming")
            self.assertEqual(kwargs["extra_env"]["STELLARIS_DISCORD_CHANNEL"], "#brainstorming")
            self.assertTrue(any("Brainstorm" in msg and "brainstorm ok" in msg for msg in ctx.sent))
            self.assertFalse(tasks_path.exists())

    async def test_ask_remains_universal_in_analysis_channel(self):
        bot = load_bot()
        calls = []

        async def fake_backend(question, **kwargs):
            calls.append((question, kwargs))
            return "ask ok", None

        bot.run_ask_backend = fake_backend
        ctx = FakeCtx()
        ctx.channel.name = "analysis"
        ctx.channel.category = None

        await bot.cmd_ask(ctx, question="plain ask")

        self.assertEqual(calls, [("plain ask", {})])
        self.assertTrue(any("Ask" in msg and "ask ok" in msg for msg in ctx.sent))

    async def test_run_in_conversation_channels_returns_guidance_without_task_write(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})
            calls = []

            async def fake_intake(*args, **kwargs):
                calls.append(("intake", args, kwargs))
                return None, None

            def fake_append(*args, **kwargs):
                calls.append(("append", args, kwargs))

            bot.intake_github_work = fake_intake
            bot.append_task_locked = fake_append

            analysis_ctx = FakeCtx()
            analysis_ctx.channel.name = "analysis"
            await bot.cmd_run(analysis_ctx, request="ship")

            brainstorming_ctx = FakeCtx()
            brainstorming_ctx.channel.name = "brainstorming"
            await bot.cmd_run(brainstorming_ctx, request="ship")

            self.assertTrue(any("!analyze" in msg for msg in analysis_ctx.sent))
            self.assertTrue(any("!brainstorm" in msg for msg in brainstorming_ctx.sent))
            self.assertFalse(tasks_path.exists())
            self.assertEqual(calls, [])

    async def test_run_generic_non_pipeline_guidance_is_preserved(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            bot._projects_store.PROJECTS_JSON_PATH = str(Path(tmp) / "projects.json")
            bot._projects_store.TASKS_JSON_PATH = str(Path(tmp) / "tasks.json")
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})
            ctx = FakeCtx()
            ctx.channel.name = "general"

            await bot.cmd_run(ctx, request="ship")

            self.assertTrue(any("#planning" in msg and "#development" in msg and "#review" in msg for msg in ctx.sent))
            self.assertFalse((Path(tmp) / "tasks.json").exists())

    async def test_run_persists_v1_core_contract_without_live_github_mutation(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / ".canopus"
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot = load_bot(
                CANOPUS_GITHUB_PROJECT_MODE="mutate-live",
                CANOPUS_STATE_PATH=str(state),
            )
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})
            ctx = FakeCtx()
            ctx.channel.name = "development"

            await bot.cmd_run(ctx, request="implement status summary")

            task = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            payload = json.loads(task["payload"])
            self.assertEqual(task["meta"]["status"], "Pending")
            self.assertEqual(task["meta"]["job_id"], payload["job_id"])
            self.assertEqual(task["meta"]["intent"], bot.CODE_CHANGE)
            self.assertEqual(payload["intent"], bot.CODE_CHANGE)
            self.assertEqual(payload["job_status"], "classified")
            self.assertEqual(payload["runner_backend"], "canopus")
            self.assertEqual(payload["discord_context_id"], "discord-message-1-2-3")
            self.assertEqual(payload["follow_up_user_id"], "4")
            self.assertEqual(payload["artifact_root"], str(state / "artifacts" / payload["canopus_run_id"]))
            self.assertEqual(payload["artifact_paths"]["finalize_result"], str(state / "runs" / f"{payload['canopus_run_id']}-finalize.txt"))
            self.assertEqual(payload["canopus_mutation_mode_projection"], "discord-v1-live-gate-closed")
            self.assertEqual(payload["github_mutation_gate"], "closed")
            self.assertFalse(payload["github_push_ready"])
            self.assertFalse(payload["draft_pr_ready"])
            self.assertNotIn("github_project_mode", payload)
            self.assertNotIn("github_issue_url", payload)
            self.assertTrue(any("Task 추가됨" in msg for msg in ctx.sent))

    async def test_run_creates_discord_task_thread_and_persists_context(self):
        with tempfile.TemporaryDirectory() as tmp:
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot = load_bot()
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})

            class Thread:
                id = 99
                parent_id = 2
                mention = "<#99>"

                def __init__(self):
                    self.sent = []

                async def send(self, message):
                    self.sent.append(message)

            thread = Thread()

            class Message:
                id = 3

                async def create_thread(self, **kwargs):
                    self.thread_kwargs = kwargs
                    return thread

            ctx = FakeCtx()
            ctx.message = Message()

            await bot.cmd_run(ctx, request="implement status summary")

            task = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            payload = json.loads(task["payload"])
            self.assertEqual(payload["discord_thread_id"], "99")
            self.assertEqual(payload["discord_parent_channel_id"], "2")
            self.assertEqual(payload["discord_context_kind"], "thread")
            self.assertEqual(payload["discord_context_id"], "discord-thread-99")
            self.assertEqual(task["meta"]["discord_thread_id"], "99")
            self.assertEqual(task["meta"]["discord_context_id"], "discord-thread-99")
            self.assertTrue(any("Thread" in msg and "<#99>" in msg for msg in ctx.sent))
            self.assertTrue(any("Job session 생성됨" in msg for msg in thread.sent))

    async def test_run_continues_when_discord_task_thread_creation_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot = load_bot()
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})

            class Message:
                id = 3

                async def create_thread(self, **kwargs):
                    raise bot.discord.Forbidden("missing permissions")

            ctx = FakeCtx()
            ctx.message = Message()

            await bot.cmd_run(ctx, request="implement status summary")

            task = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            payload = json.loads(task["payload"])
            self.assertEqual(payload["discord_context_kind"], "message")
            self.assertNotIn("discord_thread_id", payload)
            self.assertNotIn("discord_thread_id", task["meta"])
            self.assertTrue(any("thread를 만들지 못했습니다" in msg for msg in ctx.sent))

    async def test_approve_persists_before_invoking_finalize_and_reports_commit(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            task = {
                "task_id": "discord-1",
                "task_type": {"Custom": "canopus.agent"},
                "payload": json.dumps({"agenda_id": "agenda-discord-1", "approval_state": "pending"}),
                "meta": {"status": "PendingReview"},
            }
            tasks_path = configure_project(bot, tmp, task)
            calls = []

            async def fake_finalize(path, task_id):
                calls.append((path, task_id))
                stored = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
                self.assertEqual(stored["meta"]["status"], "Processed")
                self.assertEqual(json.loads(stored["payload"])["approval_state"], "approved")
                return {
                    "ok": True,
                    "status": "finalized",
                    "task_id": task_id,
                    "branch": "canopus/agenda-discord-1-discord-1",
                    "commit": "abc123",
                    "token_usage": {
                        "input_tokens": 12345,
                        "output_tokens": 678,
                        "total_tokens": 13023,
                    },
                }, None

            bot.finalize_approved_task = fake_finalize
            ctx = FakeCtx()

            await bot.cmd_approve(ctx, task_id="discord-1")

            self.assertEqual(calls, [(str(tasks_path), "discord-1")])
            self.assertIn("태스크 승인됨", ctx.sent[0])
            self.assertIn("finalized", ctx.sent[0])
            self.assertIn("canopus/agenda-discord-1-discord-1", ctx.sent[0])
            self.assertIn("abc123", ctx.sent[0])
            self.assertIn("13,023 total", ctx.sent[0])
            self.assertIn("input 12.3k / output 0.7k", ctx.sent[0])

    async def test_approve_failure_does_not_roll_back_and_reports_retry_hint(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            task = {
                "task_id": "discord-1",
                "task_type": {"Custom": "canopus.agent"},
                "payload": json.dumps({"agenda_id": "agenda-discord-1", "approval_state": "pending"}),
                "meta": {"status": "PendingReview"},
            }
            tasks_path = configure_project(bot, tmp, task)

            async def fake_finalize(path, task_id):
                return {
                    "ok": False,
                    "status": "failed",
                    "task_id": task_id,
                    "retryable": True,
                    "error": {"code": "git_commit_failed", "message": "commit failed", "retryable": True, "details": {}},
                }, "stderr ignored for parsing"

            bot.finalize_approved_task = fake_finalize
            ctx = FakeCtx()

            await bot.cmd_approve(ctx, task_id="discord-1")

            stored = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            self.assertEqual(stored["meta"]["status"], "Processed")
            self.assertEqual(json.loads(stored["payload"])["approval_state"], "approved")
            self.assertIn("태스크 승인됨", ctx.sent[0])
            self.assertIn("failed", ctx.sent[0])
            self.assertIn("commit failed", ctx.sent[0])
            self.assertIn("!finalize discord-1", ctx.sent[0])

    async def test_approve_without_canopus_command_keeps_approval_and_reports_not_configured(self):
        bot = load_bot(CANOPUS_COMMAND="")
        with tempfile.TemporaryDirectory() as tmp:
            task = {
                "task_id": "discord-1",
                "payload": json.dumps({"agenda_id": "agenda-discord-1", "approval_state": "pending"}),
                "meta": {"status": "PendingReview"},
            }
            tasks_path = configure_project(bot, tmp, task)
            ctx = FakeCtx()

            await bot.cmd_approve(ctx, task_id="discord-1")

            stored = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            self.assertEqual(stored["meta"]["status"], "Processed")
            self.assertIn("not attempted", ctx.sent[0])
            self.assertIn("CANOPUS_COMMAND", ctx.sent[0])

    async def test_finalize_retries_approved_processed_task(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            task = {
                "task_id": "discord-1",
                "payload": json.dumps({"agenda_id": "agenda-discord-1", "approval_state": "approved"}),
                "meta": {"status": "Processed", "approval_state": "approved"},
            }
            tasks_path = configure_project(bot, tmp, task)
            calls = []

            async def fake_finalize(path, task_id):
                calls.append((path, task_id))
                return {"ok": True, "status": "no_changes", "task_id": task_id}, None

            bot.finalize_approved_task = fake_finalize
            ctx = FakeCtx()

            await bot.cmd_finalize(ctx, task_id="discord-1")

            self.assertEqual(calls, [(str(tasks_path), "discord-1")])
            self.assertIn("최종화 재시도", ctx.sent[0])
            self.assertIn("no changes", ctx.sent[0])

    async def test_repeated_approve_on_already_approved_guides_to_finalize(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            task = {
                "task_id": "discord-1",
                "payload": json.dumps({"agenda_id": "agenda-discord-1", "approval_state": "approved"}),
                "meta": {"status": "Processed", "approval_state": "approved"},
            }
            configure_project(bot, tmp, task)
            ctx = FakeCtx()

            await bot.cmd_approve(ctx, task_id="discord-1")

            self.assertIn("!finalize discord-1", ctx.sent[0])

    async def test_intake_github_work_runs_for_issue_only_registration(self):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / "fake_canopus.py"
            argv_path = Path(tmp) / "argv.json"
            script.write_text(
                "import json, pathlib, sys\n"
                f"pathlib.Path({str(argv_path)!r}).write_text(json.dumps(sys.argv), encoding='utf-8')\n"
                "print(json.dumps({'ok': True, 'github_issue_number': 7, 'github_issue_url': 'https://github.test/acme/demo/issues/7'}))\n",
                encoding="utf-8",
            )
            bot = load_bot(CANOPUS_COMMAND=f"{sys.executable} {script}")

            result, error = await bot.intake_github_work(
                {"repo_path": "/repo", "github_owner": "acme", "github_repo": "demo"},
                "discord-1",
                "agenda-discord-1",
                "ship issue",
                "https://discord.test/message",
            )

            self.assertIsNone(error)
            self.assertEqual(result["github_issue_number"], 7)
            argv = json.loads(argv_path.read_text(encoding="utf-8"))
            self.assertIn("--project-sync", argv)
            self.assertIn("best-effort", argv)
            self.assertNotIn("github_project_id", json.loads(argv[argv.index("--registration") + 1]))

    async def test_intake_github_work_skips_without_owner_repo(self):
        bot = load_bot(CANOPUS_COMMAND=f"{sys.executable} -c 'raise SystemExit(99)'")

        result, error = await bot.intake_github_work(
            {"repo_path": "/repo", "github_project_id": "PVT_1"},
            "discord-1",
            "agenda-discord-1",
            "ship issue",
            None,
        )

        self.assertIsNone(result)
        self.assertIsNone(error)

    async def test_register_github_project_uses_canopus_json_boundary(self):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / "fake_canopus.py"
            script.write_text(
                "import json, sys\n"
                "assert sys.argv[1] == 'project-register'\n"
                "print(json.dumps({'github_owner':'acme','github_repo':'demo','github_project_id':'PVT_1','github_home_issue_url':'https://github.test/i/1'}))\n",
                encoding="utf-8",
            )
            bot = load_bot(CANOPUS_COMMAND=f"{sys.executable} {script}")
            result, error = await bot.register_github_project(
                "/repo",
                {
                    "github_owner": "acme",
                    "github_repo": "demo",
                    "github_project_owner_kind": "org",
                    "github_project_owner": "acme",
                    "create_github_repo": False,
                },
            )
            self.assertIsNone(error)
            self.assertEqual(result["github_project_id"], "PVT_1")

    async def test_run_work_intake_failure_does_not_append_task(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo", "github_project_id": "PVT_1"}}})

            async def fail_intake(*args, **kwargs):
                return None, "mock intake failed"

            bot.intake_github_work = fail_intake

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(
                        id=2,
                        name="development",
                        category=types.SimpleNamespace(id=10),
                    )
                    self.message = types.SimpleNamespace(id=3)
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_run(ctx, request="ship")
            self.assertFalse(tasks_path.exists())
            self.assertTrue(any("work-intake 실패" in msg for msg in ctx.sent))

    async def test_propose_approve_intake_failure_keeps_pending_proposal(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo", "github_project_id": "PVT_1"}}})
            task = {
                "task_id": "discord-proposal",
                "payload": json.dumps({"request": "candidate", "agenda_id": "agenda-discord-proposal"}),
                "meta": {"status": "PendingProposal"},
            }
            tasks_path.write_text(json.dumps([task]), encoding="utf-8")

            async def fail_intake(*args, **kwargs):
                return None, "mock intake failed"

            bot.intake_github_work = fail_intake

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(id=2, name="development", category=types.SimpleNamespace(id=10))
                    self.message = types.SimpleNamespace(id=3)
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_propose_approve(ctx, task_id="discord-proposal")
            stored = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            self.assertEqual(stored["meta"]["status"], "PendingProposal")
            payload = json.loads(stored["payload"])
            self.assertEqual(payload["proposal_intake_state"], "failed")
            self.assertTrue(any("work-intake 실패" in msg for msg in ctx.sent))

    async def test_propose_approve_happy_path_transitions_to_pending(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects(
                {
                    "projects": {
                        "10": {
                            "name": "demo",
                            "repo_path": "/repo",
                            "github_project_id": "PVT_1",
                        }
                    }
                }
            )
            task = {
                "task_id": "discord-proposal",
                "payload": json.dumps(
                    {
                        "request": "candidate",
                        "agenda_id": "agenda-discord-proposal",
                        "discord_message_url": "https://discord.test/message",
                    }
                ),
                "meta": {"status": "PendingProposal"},
            }
            tasks_path.write_text(json.dumps([task]), encoding="utf-8")

            async def succeed_intake(project, task_id, agenda_id, request, message_url):
                self.assertEqual(project["github_project_id"], "PVT_1")
                self.assertEqual(task_id, "discord-proposal")
                self.assertEqual(agenda_id, "agenda-discord-proposal")
                self.assertEqual(request, "candidate")
                self.assertEqual(message_url, "https://discord.test/message")
                return {
                    "github_issue_number": 42,
                    "github_issue_url": "https://github.test/acme/demo/issues/42",
                    "github_project_item_id": "PVTI_1",
                }, None

            bot.intake_github_work = succeed_intake

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(
                        id=2,
                        name="development",
                        category=types.SimpleNamespace(id=10),
                    )
                    self.message = types.SimpleNamespace(id=3)
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_propose_approve(ctx, task_id="discord-proposal")

            stored = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            self.assertEqual(stored["meta"]["status"], "Pending")
            self.assertEqual(stored["meta"]["proposal_intake_state"], "succeeded")
            payload = json.loads(stored["payload"])
            self.assertEqual(payload["proposal_intake_state"], "succeeded")
            self.assertEqual(payload["github_issue_number"], 42)
            self.assertEqual(payload["github_project_item_id"], "PVTI_1")
            self.assertTrue(any("후보 승인됨" in msg for msg in ctx.sent))

    async def test_worktree_command_reports_clean_registered_repo(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            import subprocess
            subprocess.run(["git", "init"], cwd=repo, check=True, capture_output=True, text=True)
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": str(repo)}}})

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(
                        id=2,
                        name="development",
                        category=types.SimpleNamespace(id=10),
                    )
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_worktree(ctx)

            self.assertEqual(len(ctx.sent), 1)
            self.assertIn("Worktree 상태", ctx.sent[0])
            self.assertIn("clean", ctx.sent[0])


    def test_normalize_project_worktrees_hydrates_legacy_record(self):
        bot = load_bot()

        project = bot.normalize_project_worktrees({"name": "demo", "repo_path": "/repo"})

        self.assertEqual(project["base_repo_path"], "/repo")
        self.assertEqual(project["active_worktree"], "default")
        self.assertEqual(project["worktrees"]["default"]["repo_path"], "/repo")

    async def test_worktree_list_reports_legacy_default(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(id=2, name="development", category=types.SimpleNamespace(id=10))
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_worktree(ctx, action="list")

            self.assertIn("Worktree 목록", ctx.sent[0])
            self.assertIn("`default`", ctx.sent[0])
            self.assertIn("`/repo`", ctx.sent[0])

    async def test_worktree_create_records_canopus_result_only_on_success(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/base"}}})
            calls = []

            async def fake_create(base_repo_path, name, target_path=None):
                calls.append((base_repo_path, name, target_path))
                return {"status": "created", "name": name, "repo_path": "/base-smoke"}, None

            bot.create_worktree = fake_create

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(id=2, name="development", category=types.SimpleNamespace(id=10))
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_worktree(ctx, action="create", name="smoke")

            self.assertEqual(calls, [("/base", "smoke", None)])
            stored = bot.read_projects()["projects"]["10"]
            self.assertEqual(stored["worktrees"]["smoke"]["repo_path"], "/base-smoke")
            self.assertEqual(stored["repo_path"], "/base")
            self.assertIn("worktree 생성됨", ctx.sent[0])

    async def test_worktree_create_rejects_unsafe_or_duplicate_without_canopus(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects(
                {"projects": {"10": {"name": "demo", "repo_path": "/base", "worktrees": {"smoke": {"repo_path": "/base-smoke"}}}}}
            )
            calls = []

            async def fake_create(*args, **kwargs):
                calls.append(args)
                return None, "should not be called"

            bot.create_worktree = fake_create

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(id=2, name="development", category=types.SimpleNamespace(id=10))
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            bad = Ctx()
            await bot.cmd_worktree(bad, action="create", name="../bad")
            duplicate = Ctx()
            await bot.cmd_worktree(duplicate, action="create", name="smoke")

            self.assertEqual(calls, [])
            self.assertIn("사용할 수 없습니다", bad.sent[0])
            self.assertIn("이미 등록된", duplicate.sent[0])

    async def test_worktree_create_failure_does_not_update_project_state(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/base"}}})

            async def fake_create(*args, **kwargs):
                return None, "git failed"

            bot.create_worktree = fake_create

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(id=2, name="development", category=types.SimpleNamespace(id=10))
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_worktree(ctx, action="create", name="smoke")

            stored = bot.read_projects()["projects"]["10"]
            self.assertNotIn("smoke", bot.normalize_project_worktrees(stored)["worktrees"])
            self.assertIn("생성 실패", ctx.sent[0])

    async def test_worktree_switch_updates_future_run_payload_repo(self):
        bot = load_bot()
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp) / "base"
            smoke = Path(tmp) / "base-smoke"
            base.mkdir()
            smoke.mkdir()
            import subprocess
            subprocess.run(["git", "init"], cwd=base, check=True, capture_output=True, text=True)
            subprocess.run(["git", "init"], cwd=smoke, check=True, capture_output=True, text=True)
            projects_path = Path(tmp) / "projects.json"
            tasks_path = Path(tmp) / "tasks.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects(
                {
                    "projects": {
                        "10": {
                            "name": "demo",
                            "repo_path": str(base),
                            "base_repo_path": str(base),
                            "active_worktree": "default",
                            "worktrees": {
                                "default": {"repo_path": str(base)},
                                "smoke": {"repo_path": str(smoke)},
                            },
                        }
                    }
                }
            )

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(id=2, name="development", category=types.SimpleNamespace(id=10))
                    self.message = types.SimpleNamespace(id=3)
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            switch_ctx = Ctx()
            await bot.cmd_worktree(switch_ctx, action="switch", name="smoke")
            run_ctx = Ctx()
            await bot.cmd_run(run_ctx, request="update smoke file")

            stored_project = bot.read_projects()["projects"]["10"]
            self.assertEqual(stored_project["repo_path"], str(smoke))
            task = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            payload = json.loads(task["payload"])
            self.assertEqual(payload["repo_path"], str(smoke))
            self.assertEqual(payload["worktree_name"], "smoke")
            self.assertEqual(payload["worktree_repo_path"], str(smoke))
            self.assertEqual(payload["worktree_readiness"], "selected")
            self.assertEqual(task["meta"]["job_id"], payload["job_id"])
            self.assertEqual(task["meta"]["job_status"], "classified")
            self.assertEqual(task["meta"]["intent"], bot.CODE_CHANGE)
            self.assertEqual(task["meta"]["classification_reason"], "explicit_code_change")
            self.assertIn("active worktree 변경됨", switch_ctx.sent[0])

    async def test_show_includes_discord_identity_and_finalize_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / ".canopus"
            runs = state / "runs"
            runs.mkdir(parents=True)
            (runs / "agenda-discord-show-finalize.txt").write_text("finalized\n", encoding="utf-8")
            (runs / "agenda-discord-show-delivery-gate.json").write_text(
                '{"status":"Denied"}\n',
                encoding="utf-8",
            )
            bot = load_bot(CANOPUS_STATE_PATH=str(state))
            tasks_path = Path(tmp) / "tasks.json"
            projects_path = Path(tmp) / "projects.json"
            bot._projects_store.PROJECTS_JSON_PATH = str(projects_path)
            bot._projects_store.TASKS_JSON_PATH = str(tasks_path)
            bot.write_projects({"projects": {"10": {"name": "demo", "repo_path": "/repo"}}})
            task = {
                "task_id": "discord-show",
                "task_type": {"Custom": "canopus.agent"},
                "payload": json.dumps(
                    {
                        "request": "show artifacts",
                        "agenda_id": "agenda-discord-show",
                        "discord_channel_id": "2",
                        "discord_message_id": "3",
                    }
                ),
                "meta": {"status": "Processed"},
            }
            tasks_path.write_text(json.dumps([task]), encoding="utf-8")

            class Ctx:
                def __init__(self):
                    self.sent = []
                    self.guild = types.SimpleNamespace(id=1)
                    self.channel = types.SimpleNamespace(
                        id=2,
                        name="development",
                        category=types.SimpleNamespace(id=10),
                    )
                    self.message = types.SimpleNamespace(id=3)
                    self.author = types.SimpleNamespace(id=4)

                async def send(self, message):
                    self.sent.append(message)

            ctx = Ctx()
            await bot.cmd_show(ctx, task_id="discord-show")

            self.assertEqual(len(ctx.sent), 1)
            output = ctx.sent[0]
            self.assertIn("discord_channel_id: 2", output)
            self.assertIn("discord_message_id: 3", output)
            self.assertIn("agenda-discord-show-finalize.txt", output)
            self.assertIn("agenda-discord-show-delivery-gate.json", output)


if __name__ == "__main__":
    unittest.main()
