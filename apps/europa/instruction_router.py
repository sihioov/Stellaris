"""Rule-based Discord request intent classification."""
from dataclasses import dataclass
import re


READ_ONLY_ANALYSIS = "ReadOnlyAnalysis"
CODE_CHANGE = "CodeChange"
PR_REVIEW = "PrReview"
CI_REPAIR = "CiRepair"
NEEDS_CLARIFICATION = "NeedsClarification"


@dataclass(frozen=True)
class InstructionClassification:
    intent: str
    reason: str


_OBJECTLESS_COMMANDS = frozenset(
    {
        "do it",
        "fix it",
        "handle it",
        "help",
        "help me",
        "make it work",
        "작업해줘",
        "처리해줘",
        "해줘",
        "고쳐줘",
        "수정해줘",
    }
)

_CI_TERMS = (
    "ci",
    "github action",
    "github actions",
    "workflow",
    "build",
    "test failure",
    "action 실패",
    "빌드 실패",
    "테스트 실패",
)

_CI_FAILURE_TERMS = (
    "broken",
    "fail",
    "failed",
    "failure",
    "fix",
    "repair",
    "red",
    "깨졌",
    "고쳐",
    "수리",
    "실패",
)

_CODE_CHANGE_TERMS = (
    "add",
    "build",
    "change",
    "create",
    "fix",
    "implement",
    "refactor",
    "remove",
    "update",
    "write",
    "고쳐",
    "구현",
    "만들",
    "바꿔",
    "삭제",
    "수정",
    "작성",
    "추가",
)

_READ_ONLY_TERMS = (
    "analyze",
    "check",
    "describe",
    "explain",
    "find",
    "inspect",
    "investigate",
    "look at",
    "review",
    "show",
    "where",
    "검토",
    "분석",
    "설명",
    "알려",
    "어디",
    "조사",
    "찾아",
    "확인",
)


def _normalize(request: str | None) -> str:
    return " ".join((request or "").strip().lower().split())


def _contains_any(text: str, terms: tuple[str, ...]) -> bool:
    return any(term in text for term in terms)


def _is_pr_review(text: str) -> bool:
    return bool(
        re.search(r"\bpr\s*#?\d+\b", text)
        or re.search(r"\bpull request\b", text)
        or "pr 리뷰" in text
        or "풀리퀘" in text
    )


def _is_ci_repair(text: str) -> bool:
    return _contains_any(text, _CI_TERMS) and _contains_any(text, _CI_FAILURE_TERMS)


def classify_instruction(request: str | None) -> InstructionClassification:
    text = _normalize(request)
    if not text:
        return InstructionClassification(NEEDS_CLARIFICATION, "empty_request")
    if text in _OBJECTLESS_COMMANDS:
        return InstructionClassification(NEEDS_CLARIFICATION, "objectless_command")
    if _is_ci_repair(text):
        return InstructionClassification(CI_REPAIR, "ci_failure_repair")
    if _is_pr_review(text):
        return InstructionClassification(PR_REVIEW, "pr_review_reference")
    if _contains_any(text, _CODE_CHANGE_TERMS):
        return InstructionClassification(CODE_CHANGE, "explicit_code_change")
    if _contains_any(text, _READ_ONLY_TERMS):
        return InstructionClassification(READ_ONLY_ANALYSIS, "read_only_analysis")
    return InstructionClassification(NEEDS_CLARIFICATION, "no_stable_intent_match")
