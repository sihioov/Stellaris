# Stellaris AI Pipeline Launcher

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

$TASKS_JSON  = Join-Path $REPO_ROOT "tasks.json"
$WEBHOOK_URL = $env:DISCORD_WEBHOOK_URL
$BOT_ENV     = Join-Path $REPO_ROOT "apps\discord-bot\.env"

# 검증
if (-not $WEBHOOK_URL) {
    Write-Host "❌ .env에 DISCORD_WEBHOOK_URL이 없습니다." -ForegroundColor Red; exit 1
}
if (-not (Test-Path $BOT_ENV) -or -not (Select-String -Path $BOT_ENV -Pattern "DISCORD_BOT_TOKEN=.+")) {
    Write-Host "❌ apps/discord-bot/.env에 DISCORD_BOT_TOKEN이 없습니다." -ForegroundColor Red; exit 1
}

# Canopus 바이너리 PATH 추가
$env:PATH = "$REPO_ROOT\target\release;$env:PATH"

Write-Host "🚀 Stellaris 파이프라인 시작..." -ForegroundColor Green

# TON618
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT'; `$env:TASKS_JSON_PATH='$TASKS_JSON'; `$env:RUST_LOG='info'; cargo run -p ton618"
) -WindowStyle Normal

Start-Sleep -Seconds 2

# Laniakea
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT'; `$env:TASKS_JSON_PATH='$TASKS_JSON'; `$env:LANIAKEA_SOURCE='file'; `$env:DISCORD_WEBHOOK_URL='$WEBHOOK_URL'; `$env:CANOPUS_REPO_PATH='$REPO_ROOT'; `$env:CANOPUS_STATE_PATH='$REPO_ROOT\.canopus'; `$env:RUST_LOG='info'; cargo run -p laniakea"
) -WindowStyle Normal

Start-Sleep -Seconds 2

# Discord Bot
Start-Process powershell -ArgumentList @(
    "-NoExit", "-Command",
    "cd '$REPO_ROOT\apps\discord-bot'; pip install -r requirements.txt -q; python bot.py"
) -WindowStyle Normal

Write-Host ""
Write-Host "✅ 3개 프로세스 시작됨:" -ForegroundColor Green
Write-Host "   🔵 TON618      — 태스크 스케줄러 (10초 폴링)"
Write-Host "   🟠 Laniakea    — AI 워커 (Canopus 실행)"
Write-Host "   🟡 Discord Bot — !run / !approve / !reject"
Write-Host ""
Write-Host "Discord에서 !run <요청> 으로 파이프라인을 시작하세요." -ForegroundColor Cyan
