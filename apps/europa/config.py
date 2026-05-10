"""Configuration and static maps for the Stellaris Discord bot."""
import os

from dotenv import load_dotenv

load_dotenv()

DISCORD_BOT_TOKEN = os.environ.get("DISCORD_BOT_TOKEN")
PROJECTS_JSON_PATH = os.environ.get(
    "PROJECTS_JSON_PATH",
    os.path.join(os.path.dirname(__file__), "projects.json"),
)
TASKS_DIR = os.environ.get("TASKS_DIR", os.path.dirname(__file__))
TASKS_JSON_PATH = os.environ.get("TASKS_JSON_PATH", "").strip()
CANOPUS_STATE_PATH = os.environ.get("CANOPUS_STATE_PATH")
_NEW_PROJECT_DEFAULT_ROOT = os.environ.get("NEW_PROJECT_DEFAULT_ROOT", "").strip() or "/home/sihioov/project"
NEW_PROJECT_DEFAULT_ROOT = os.path.abspath(os.path.expanduser(_NEW_PROJECT_DEFAULT_ROOT))
CANOPUS_COMMAND = os.environ.get("CANOPUS_COMMAND", "canopus").strip()
ASK_COMMAND = os.environ.get("ASK_COMMAND", "").strip()
GITHUB_OWNER = os.environ.get("GITHUB_OWNER", "").strip()
GITHUB_REPO = os.environ.get("GITHUB_REPO", "").strip()
GITHUB_PROJECT_ID = os.environ.get("GITHUB_PROJECT_ID", "").strip()
GITHUB_PROJECT_URL = os.environ.get("GITHUB_PROJECT_URL", "").strip()
GITHUB_PROJECT_OWNER_KIND = os.environ.get("GITHUB_PROJECT_OWNER_KIND", "").strip()
GITHUB_PROJECT_OWNER = os.environ.get("GITHUB_PROJECT_OWNER", "").strip()
GITHUB_PROJECT_NUMBER = os.environ.get("GITHUB_PROJECT_NUMBER", "").strip()
GITHUB_PROJECT_STATUS_FIELD_ID = os.environ.get("GITHUB_PROJECT_STATUS_FIELD_ID", "").strip()
GITHUB_PROJECT_STATUS_FIELD_NAME = os.environ.get("GITHUB_PROJECT_STATUS_FIELD_NAME", "").strip()
GITHUB_PROJECT_STATUS_OPTION_ID = os.environ.get("GITHUB_PROJECT_STATUS_OPTION_ID", "").strip()
GITHUB_PROJECT_STATUS_OPTION_NAME = os.environ.get("GITHUB_PROJECT_STATUS_OPTION_NAME", "").strip()
CANOPUS_GITHUB_PROJECT_MODE = os.environ.get("CANOPUS_GITHUB_PROJECT_MODE", "").strip()
NON_MUTATING_GITHUB_PROJECT_MODES = {"dry-run-offline", "validate-read-only"}


def env_int(name: str, default: int, minimum: int, maximum: int) -> int:
    try:
        value = int(os.environ.get(name, str(default)))
    except ValueError:
        return default
    return max(minimum, min(maximum, value))


ASK_TIMEOUT_SECONDS = env_int("ASK_TIMEOUT_SECONDS", 30, 1, 300)
ASK_MAX_OUTPUT_CHARS = env_int("ASK_MAX_OUTPUT_CHARS", 1800, 200, 1800)

_raw_ids = os.environ.get("ALLOWED_USER_IDS", "")
ALLOWED_USER_IDS: set = {
    int(uid.strip()) for uid in _raw_ids.split(",") if uid.strip().isdigit()
}

CHANNEL_TYPE_MAP = {
    "planning": "canopus.planner",
    "development": "canopus.agent",
    "review": "canopus.reviewer",
    "general": None,
    "analysis": None,
    "brainstorming": None,
}

ICON_MAP = {
    "Pending": "⏳",
    "Dispatched": "🚀",
    "PendingReview": "🔍",
    "Processed": "✅",
    "Failed": "❌",
    "PendingProposal": "📝",
}


def get_channel_type(channel_name: str) -> str | None:
    name = channel_name.lower().strip()
    return CHANNEL_TYPE_MAP.get(name)


def is_authorized(ctx) -> bool:
    if not ALLOWED_USER_IDS:
        return True
    return ctx.author.id in ALLOWED_USER_IDS
