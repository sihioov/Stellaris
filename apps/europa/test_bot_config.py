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


if __name__ == "__main__":
    unittest.main()

class DiscordBotGitHubBoundaryTests(unittest.IsolatedAsyncioTestCase):
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
