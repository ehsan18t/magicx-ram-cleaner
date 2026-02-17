#!/usr/bin/env pwsh
# MagicX RAM Cleaner — Install git hooks
# Run once after cloning: .\scripts\install-hooks.ps1

$ErrorActionPreference = "Stop"

$hookSource = Join-Path $PSScriptRoot ".." "hooks" "pre-commit"
$hookTarget = Join-Path $PSScriptRoot ".." ".git" "hooks" "pre-commit"

if (Test-Path $hookTarget) {
    Write-Host "Pre-commit hook already installed at: $hookTarget" -ForegroundColor Yellow
    Write-Host "Overwriting..."
}

Copy-Item $hookSource $hookTarget -Force
Write-Host "Pre-commit hook installed successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "Quality gates will now run automatically before every commit:" -ForegroundColor Cyan
Write-Host "  1. cargo fmt --check    (formatting)"
Write-Host "  2. cargo clippy         (lints)"
Write-Host "  3. cargo test           (tests)"
