# Reproducible MVP release qualification script.
# Stops on first failure and records command, exit code, tool versions, git
# state, artifact paths/sizes/hashes, and timestamps. This script NEVER
# writes cleanInstallPassed: true - clean-install evidence is manual only.

param(
  [switch]$SkipInstall,
  [switch]$SkipBundle,
  [string]$EvidenceDate = (Get-Date -Format 'yyyy-MM-dd')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
$logDir = Join-Path $repoRoot 'docs\release-evidence\logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

$timestamp = Get-Date -Format 'o'
$gates = New-Object System.Collections.Generic.List[object]

function Invoke-Gate {
  param([string]$Name, [scriptblock]$Action)
  Write-Host "== gate: $Name =="
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  # Native tools write progress/warnings to stderr; with the script-level
  # Stop preference those become terminating errors in Windows PowerShell
  # 5.1, so relax the preference inside gate execution only.
  $previousPreference = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $output = & $Action 2>&1
    $exit = $LASTEXITCODE
    if ($null -eq $exit) { $exit = 0 }
  } catch {
    $output = $_.Exception.Message
    $exit = 1
  } finally {
    $ErrorActionPreference = $previousPreference
  }
  $sw.Stop()
  $logPath = Join-Path $logDir ("{0}.log" -f ($Name -replace '[^a-z0-9_-]', '_'))
  $output | Out-File -FilePath $logPath -Encoding utf8
  $gates.Add([pscustomobject]@{
    name = $Name
    exitCode = $exit
    durationSeconds = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    log = $logPath
  }) | Out-Null
  if ($exit -ne 0) {
    Write-Host "GATE FAILED: $Name (exit $exit). Log: $logPath"
    Pop-Location
    exit $exit
  }
}

$gitCommit = git rev-parse HEAD
$gitDirty = (git status --porcelain | Measure-Object -Line).Lines
$nodeVersion = node --version
$pnpmVersion = pnpm --version
$rustVersion = rustc --version
$cargoVersion = cargo --version

if (-not $SkipInstall) {
  Invoke-Gate 'pnpm-install' { pnpm install --frozen-lockfile }
}
Invoke-Gate 'pnpm-test' { pnpm test }
Invoke-Gate 'cargo-test' { cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -j 1 }
Invoke-Gate 'frontend-build' { pnpm --filter @cinematic/desktop build }
if (-not $SkipBundle) {
  Invoke-Gate 'tauri-build' { pnpm --filter @cinematic/desktop tauri build }
}
Invoke-Gate 'git-diff-check' { git diff --check }

# Documentation state machine: unqualified `MVP IMPLEMENTED` claims must not
# appear in user-facing docs while clean-install evidence is pending.
Invoke-Gate 'docs-status-check' {
  $violations = @()
  foreach ($doc in @('README.md', 'docs/architecture.md')) {
    $text = Get-Content (Join-Path $repoRoot $doc) -Raw
    if ($text -match 'MVP IMPLEMENTED') {
      $violations += $doc
    }
    if ($text -notmatch 'MVP RELEASE CANDIDATE') {
      $violations += "$doc (missing MVP RELEASE CANDIDATE status)"
    }
  }
  if ($violations.Count -gt 0) {
    Write-Output ("documentation status violations: " + ($violations -join ', '))
    $global:LASTEXITCODE = 1
  } else {
    Write-Output 'documentation status: MVP RELEASE CANDIDATE (correct)'
    $global:LASTEXITCODE = 0
  }
}

# Collect bundle artifacts strictly beneath the release bundle directory.
$artifacts = New-Object System.Collections.Generic.List[object]
if (-not $SkipBundle) {
  $bundleRoot = Join-Path $repoRoot 'apps\desktop\src-tauri\target\release\bundle'
  if (Test-Path $bundleRoot) {
    Get-ChildItem -Path $bundleRoot -Recurse -File | ForEach-Object {
      $resolved = $_.FullName
      if (-not $resolved.StartsWith($bundleRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "resolved artifact escaped bundle root: $resolved"
      }
      $hash = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
      $artifacts.Add([pscustomobject]@{
        path = $resolved
        sizeBytes = $_.Length
        sha256 = $hash
      })
    }
  }
  if ($artifacts.Count -eq 0) {
    Write-Host 'GATE FAILED: no bundle artifacts found under target\release\bundle'
    Pop-Location
    exit 1
  }
}

# Emit the release-candidate evidence document.
$evidencePath = Join-Path $repoRoot "docs\release-evidence\$EvidenceDate-mvp-release-candidate.md"
$gateTable = ($gates | ForEach-Object { "| $($_.name) | $($_.exitCode) | $($_.durationSeconds)s |" }) -join "`n"
$artifactTable = if ($artifacts.Count -gt 0) {
  ($artifacts | ForEach-Object { "| ``$($_.path)`` | $($_.sizeBytes) | $($_.sha256) |" }) -join "`n"
} else {
  '| (bundle skipped by flag) | - | - |'
}

$evidence = @"
# MVP Release Candidate Evidence ($EvidenceDate)

Status: **MVP RELEASE CANDIDATE** (automated gates + production bundle pass; clean-install pending)

This document records automated release verification only. The clean-install
smoke test is a separate manual gate; until it passes with recorded evidence,
the release status remains MVP RELEASE CANDIDATE and must not be promoted to
MVP IMPLEMENTED.

## Tool versions

| tool | version |
| --- | --- |
| git commit | $gitCommit |
| dirty files at verification | $gitDirty |
| node | $nodeVersion |
| pnpm | $pnpmVersion |
| rustc | $rustVersion |
| cargo | $cargoVersion |
| started at | $timestamp |

## Automated gates

| gate | exit code | duration |
| --- | --- | --- |
$gateTable

## Bundle artifacts

| absolute path | size (bytes) | sha-256 |
| --- | --- | --- |
$artifactTable

## Clean-install gate (MANUAL - NOT PERFORMED)

- [ ] clean machine or clean OS user profile used
- [ ] installer hash verified
- [ ] install succeeded from the packaged bundle
- [ ] first launch from installed shortcut succeeded
- [ ] project created, provider configured via OS keychain
- [ ] deterministic mock workflow executed end to end
- [ ] project closed and reopened with state intact
- [ ] diagnostics exported and free of media/secrets
- [ ] application uninstalled without deleting user project data

Tester: _pending_
OS/build: _pending_
Timestamps: _pending_
Result: **NOT PERFORMED**
"@
Set-Content -LiteralPath $evidencePath -Value $evidence -Encoding utf8
Write-Host "Evidence written: $evidencePath"

Pop-Location
