# Run one safe GitHub Project v2 validate-read-only probe through Canopus submit.
[CmdletBinding()]
param(
    [string]$Repo,
    [string]$State,
    [string]$AgendaId = "validate-read-only-probe",
    [string]$Request = "validate GitHub Project read-only wiring",
    [string]$GitHubProjectItemId = $env:GITHUB_PROJECT_ITEM_ID,
    [string]$GitHubIssueNumber = $env:GITHUB_ISSUE_NUMBER
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
if (-not $Repo) { $Repo = $RepoRoot }
if (-not $State) { $State = Join-Path $Repo ".canopus" }

$probeEnvKeys = @(
    "CANOPUS_ENABLE_GITHUB",
    "CANOPUS_GITHUB_PROJECT_MODE",
    "CANOPUS_ENABLE_LIVE_MUTATIONS",
    "CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION",
    "CANOPUS_ALLOW_GITHUB_PR_MUTATION",
    "CANOPUS_ALLOW_GITHUB_MERGE",
    "CANOPUS_ALLOW_DEPLOY"
)
$savedProbeEnv = @{}
foreach ($key in $probeEnvKeys) {
    $savedProbeEnv[$key] = [System.Environment]::GetEnvironmentVariable($key, "Process")
}
function Restore-ValidateReadOnlyEnv {
    foreach ($key in $probeEnvKeys) {
        [System.Environment]::SetEnvironmentVariable($key, $savedProbeEnv[$key], "Process")
    }
}

$rootEnv = Join-Path $RepoRoot ".env"
if (Test-Path $rootEnv) {
    Get-Content $rootEnv | ForEach-Object {
        if ($_ -match "^([^#][^=]*)=(.*)$") {
            [System.Environment]::SetEnvironmentVariable($matches[1].Trim(), $matches[2].Trim(), "Process")
        }
    }
}

$missing = @()
foreach ($key in @("GITHUB_TOKEN", "GITHUB_OWNER", "GITHUB_REPO", "GITHUB_PROJECT_ID")) {
    if (-not [System.Environment]::GetEnvironmentVariable($key, "Process")) { $missing += $key }
}
if (-not $GitHubProjectItemId -and -not $GitHubIssueNumber) {
    $missing += "GITHUB_PROJECT_ITEM_ID or GITHUB_ISSUE_NUMBER"
}
if ($missing.Count -gt 0) {
    Write-Warning "Skipping validate-read-only probe; missing: $($missing -join ', ')"
    $global:LASTEXITCODE = 0
    return
}

try {
    $env:CANOPUS_ENABLE_GITHUB = "1"
    $env:CANOPUS_GITHUB_PROJECT_MODE = "validate-read-only"
    $env:CANOPUS_ENABLE_LIVE_MUTATIONS = "0"
    $env:CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION = "0"
    $env:CANOPUS_ALLOW_GITHUB_PR_MUTATION = "0"
    $env:CANOPUS_ALLOW_GITHUB_MERGE = "0"
    $env:CANOPUS_ALLOW_DEPLOY = "0"

    $canopus = if ($env:CANOPUS_COMMAND) { $env:CANOPUS_COMMAND } else { "canopus" }
    $args = @(
        "submit",
        "--repo", $Repo,
        "--state", $State,
        "--agenda-id", $AgendaId,
        "--github-project-id", $env:GITHUB_PROJECT_ID,
        "--github-project-mode", "validate-read-only"
    )
    if ($GitHubProjectItemId) { $args += @("--github-project-item-id", $GitHubProjectItemId) }
    if ($GitHubIssueNumber) { $args += @("--github-issue-number", $GitHubIssueNumber) }
    if ($env:GITHUB_PROJECT_STATUS_FIELD_ID) { $args += @("--github-project-status-field-id", $env:GITHUB_PROJECT_STATUS_FIELD_ID) }
    if ($env:GITHUB_PROJECT_STATUS_FIELD_NAME) { $args += @("--github-project-status-field-name", $env:GITHUB_PROJECT_STATUS_FIELD_NAME) }
    if ($env:GITHUB_PROJECT_STATUS_OPTION_ID) { $args += @("--github-project-status-option-id", $env:GITHUB_PROJECT_STATUS_OPTION_ID) }
    if ($env:GITHUB_PROJECT_STATUS_OPTION_NAME) { $args += @("--github-project-status-option-name", $env:GITHUB_PROJECT_STATUS_OPTION_NAME) }
    $args += $Request

    Write-Host "🔎 Running GitHub Project validate-read-only probe (GitHub reads only; local submit may create local artifacts/branch)." -ForegroundColor Cyan
    & $canopus @args
} finally {
    Restore-ValidateReadOnlyEnv
}
