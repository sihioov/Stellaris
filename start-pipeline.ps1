# Stellaris AI Pipeline Launcher
[CmdletBinding()]
param(
    [switch]$DryRun
)

$REPO_ROOT = $PSScriptRoot

# 루트 .env 로드
$rootEnv = Join-Path $REPO_ROOT ".env"
if (Test-Path $rootEnv) {
    Get-Content $rootEnv | ForEach-Object {
        if ($_ -match "^([^#][^=]*)=(.*)$") {
            [System.Environment]::SetEnvironmentVariable($matches[1].Trim(), $matches[2].Trim(), "Process")
        }
    }
}

# Shared file-backed queue: TON618 reads TASKS_JSON_PATH; Laniakea reads the same path via LANIAKEA_FILE_PATH.
$TASKS_JSON  = Join-Path $REPO_ROOT "tasks.json"
$BOT_ENV     = Join-Path $REPO_ROOT "apps\europa\.env"

# 검증
if (-not $DryRun -and -not $env:DISCORD_WEBHOOK_URL) {
    Write-Host "❌ .env에 DISCORD_WEBHOOK_URL이 없습니다." -ForegroundColor Red; exit 1
}
if (-not $DryRun -and (-not (Test-Path $BOT_ENV) -or -not (Select-String -Path $BOT_ENV -Pattern "DISCORD_BOT_TOKEN=.+"))) {
    Write-Host "❌ apps/europa/.env에 DISCORD_BOT_TOKEN이 없습니다." -ForegroundColor Red; exit 1
}

if ($DryRun) {
    Write-Host "🧪 Dry-run: validating pipeline wiring without credentials, builds, or live processes." -ForegroundColor Cyan
    Write-Host "TASKS_JSON_PATH=$TASKS_JSON (TON618)"
    Write-Host "LANIAKEA_FILE_PATH=$TASKS_JSON (same file-backed queue)"
    Write-Host "TON618: cargo run -p ton618"
    Write-Host "LANIAKEA: LANIAKEA_SOURCE=file LANIAKEA_FILE_PATH=$TASKS_JSON CANOPUS_REPO_PATH=$REPO_ROOT CANOPUS_STATE_PATH=$REPO_ROOT\.canopus cargo run -p laniakea"
    Write-Host "CANOPUS WATCH/FINALIZER: CANOPUS_REPO=$REPO_ROOT CANOPUS_STATE=$REPO_ROOT\.canopus CANOPUS_ENABLE_GITHUB=0 CANOPUS_ENABLE_LIVE_MUTATIONS=0 canopus watch $TASKS_JSON"
    Write-Host "KEPLER: REPO_PATH=$REPO_ROOT cargo run -p kepler"
    Write-Host "DISCORD BOT: python bot.py (requires DISCORD_BOT_TOKEN only outside -DryRun)"
    Write-Host "Dry-run safety: no GitHub token, Discord token, push, PR creation, GitHub Issue mutation, or GitHub Project mutation is required or reached."
    Write-Host "Live gaps: real Discord, GitHub push/PR/merge/deploy, and credentialed services are not exercised by -DryRun."
    exit 0
}

# Canopus release 바이너리 빌드 (Laniakea가 PATH에서 찾음)
Write-Host "⚙️ Canopus 릴리즈 빌드 중..." -ForegroundColor Yellow
cargo build -p canopus --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Canopus 빌드 실패" -ForegroundColor Red; exit 1
}
$env:PATH = "$REPO_ROOT\target\release;$env:PATH"

Write-Host "🚀 Stellaris 파이프라인 시작..." -ForegroundColor Green

# TON618
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT'; `$env:TASKS_JSON_PATH='$TASKS_JSON'; `$env:RUST_LOG='info'; cargo run -p ton618"
) -WindowStyle Normal

Start-Sleep -Seconds 2

# Laniakea
# DISCORD_WEBHOOK_URL은 부모 프로세스 환경에서 상속되므로 커맨드 문자열에 포함하지 않음
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT'; `$env:LANIAKEA_FILE_PATH='$TASKS_JSON'; `$env:LANIAKEA_SOURCE='file'; `$env:CANOPUS_REPO_PATH='$REPO_ROOT'; `$env:CANOPUS_STATE_PATH='$REPO_ROOT\.canopus'; `$env:CANOPUS_ENABLE_GITHUB='0'; `$env:RUST_LOG='info'; cargo run -p laniakea"
) -WindowStyle Normal


Start-Sleep -Seconds 2

# Canopus watch/finalizer (closes approved Processed tasks with dry-run finalize records)
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT'; `$env:TASKS_JSON_PATH='$TASKS_JSON'; `$env:CANOPUS_REPO='$REPO_ROOT'; `$env:CANOPUS_STATE='$REPO_ROOT\.canopus'; `$env:CANOPUS_ENABLE_GITHUB='0'; `$env:CANOPUS_ENABLE_LIVE_MUTATIONS='0'; `$env:CANOPUS_ALLOW_GITHUB_MUTATION='0'; `$env:CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION='0'; canopus watch '$TASKS_JSON'"
) -WindowStyle Normal

# Kepler (codebase discovery scanner; Hubble is reserved for external collectors)
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT'; `$env:TASKS_JSON_PATH='$TASKS_JSON'; `$env:REPO_PATH='$REPO_ROOT'; `$env:RUST_LOG='info'; cargo run -p kepler"
) -WindowStyle Normal

# Discord Bot
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT\apps\europa'; `$env:TASKS_JSON_PATH='$TASKS_JSON'; `$env:CANOPUS_STATE_PATH='$REPO_ROOT\.canopus'; pip install -r requirements.txt -q; python bot.py"
) -WindowStyle Normal

Write-Host ""
Write-Host "✅ 5개 프로세스 시작됨:" -ForegroundColor Green
Write-Host "   🔵 TON618       — 태스크 스케줄러 (10초 폴링)"
Write-Host "   🟠 Laniakea     — AI 워커 (Canopus 실행)"
Write-Host "   🟢 Canopus Watch — Processed 태스크 finalize dry-run 기록"
Write-Host "   🟣 Kepler       — 코드베이스 스캐너 (PendingProposal 등록)"
Write-Host "   🟡 Discord Bot  — !run / !approve [id] / !reject [id]"
Write-Host ""
Write-Host "Discord에서 !run <요청> 으로 파이프라인을 시작하세요." -ForegroundColor Cyan
