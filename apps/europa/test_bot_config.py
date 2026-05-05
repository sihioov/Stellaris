import importlib.util
import json
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path

BOT_PATH = Path(__file__).with_name("bot.py")
BOT_DIR = str(BOT_PATH.parent)


def load_bot(**env):
    for name in [
        "discord",
        "discord.ext",
        "discord.ext.commands",
        "dotenv",
        "config",
        "canopus_client",
        "payloads",
        "projects_store",
        "tasks_store",
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


class DiscordBotConfigTests(unittest.TestCase):
    def test_tasks_json_path_overrides_per_category_file(self):
        bot = load_bot(TASKS_JSON_PATH="/tmp/stellaris-tasks.json")
        self.assertEqual(bot.get_tasks_path(123), "/tmp/stellaris-tasks.json")

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
        self.assertEqual(payload["confirmation_state"], "requested")


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
        self.assertIsNone(payload.get("github_issue_number"))
        self.assertIsNone(payload.get("github_issue_url"))

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
                "payload": json.dumps({"agenda_id": "agenda-discord-1", "approval_state": "pending"}),
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
            stored = json.loads(tasks_path.read_text(encoding="utf-8"))[0]
            self.assertEqual(stored["meta"]["approval_state"], "approved")

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
            "ignored": "not copied",
        }

        bot.promote_pending_proposal_with_intake(task, intake)

        payload = json.loads(task["payload"])
        self.assertEqual(payload["proposal_intake_state"], "succeeded")
        self.assertIsNotNone(payload["proposal_intake_attempted_at"])
        self.assertEqual(payload["github_issue_number"], 42)
        self.assertEqual(payload["github_project_item_id"], "PVTI_1")
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

class DiscordBotGitHubBoundaryTests(unittest.IsolatedAsyncioTestCase):
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
