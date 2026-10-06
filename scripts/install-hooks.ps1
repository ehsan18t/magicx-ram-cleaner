#!/usr/bin/env pwsh
# MagicX RAM Cleaner: install git hooks
# Run once after cloning: .\scripts\install-hooks.ps1
#
# Points git at the repo's hooks/ folder (core.hooksPath) instead of copying
# files into .git/hooks, so hook updates apply without reinstalling and it
# works in worktrees too.

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
git -C $repoRoot config core.hooksPath hooks
if ($LASTEXITCODE -ne 0) { throw "git config core.hooksPath failed" }

Write-Host "Git hooks enabled (core.hooksPath = hooks)." -ForegroundColor Green
Write-Host ""
Write-Host "Quality gates will now run automatically:" -ForegroundColor Cyan
Write-Host "  commit-msg (validates the commit message format):" -ForegroundColor White
Write-Host "    Format: <type>(<scope>): <description>"
Write-Host "    Types:  feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert, enforce"
Write-Host ""
Write-Host "  pre-commit (fast checks before each commit):" -ForegroundColor White
Write-Host "    1. cargo fmt --check"
Write-Host "    2. cargo clippy -D warnings"
Write-Host "    3. cargo test"
Write-Host ""
Write-Host "  pre-push (the same gates as CI):" -ForegroundColor White
Write-Host "    1. cargo fmt --check"
Write-Host "    2. cargo clippy -D warnings"
Write-Host "    3. cargo test"
Write-Host "    4. cargo bench --no-run"
Write-Host "    5. cargo build"
Write-Host "    6. cargo doc (CI rustdoc lints)"
Write-Host "    7. cargo deny check (if installed)"
