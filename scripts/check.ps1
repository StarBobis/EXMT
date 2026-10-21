# Quality gate for EXMT Modbox. Run: bun run check
# Fails fast on any step; prints a one-line summary per step.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$py = 'C:\Users\Administrator\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe'
if (-not (Test-Path $py)) { $py = 'python' }
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) {
  $found = Get-Command cargo -ErrorAction SilentlyContinue
  if ($found) { $cargo = $found.Source } else { $cargo = $null }
}

function Step($name, $block) {
  Write-Host "== $name" -ForegroundColor Cyan
  & $block
  if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL $name (exit $LASTEXITCODE)" -ForegroundColor Red
    exit $LASTEXITCODE
  }
  Write-Host "ok   $name" -ForegroundColor Green
}

Step 'frontend types' {
  Push-Location $root
  try {
    # `npx --no-install` cannot see the shims bun writes on Windows (`.exe`
    # only, no `.cmd`), so run the package entry point through node when it is
    # present and keep npx as the fallback for npm-installed trees.
    $entry = Join-Path $root 'node_modules\vue-tsc\bin\vue-tsc.js'
    if (Test-Path $entry) {
      & node $entry --noEmit
    } else {
      & npx --no-install vue-tsc --noEmit
    }
  } finally { Pop-Location }
}

Step 'rust build' {
  # Half the shell is Rust; a broken lib.rs must fail the gate, not the app.
  $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
  if (-not (Test-Path $cargo)) {
    $found = Get-Command cargo -ErrorAction SilentlyContinue
    if ($found) { $cargo = $found.Source } else { $cargo = $null }
  }
  if (-not $cargo) {
    Write-Host 'skip: cargo not found'
  } else {
    Push-Location (Join-Path $root 'src-tauri')
    try { & $cargo check --quiet } finally { Pop-Location }
  }
}

Step 'core-rs tests' {
  # The self-hosted Rust asset core. Its test suite is the contract for the
  # Unity format work, so a red test must fail the gate, not just log.
  if (-not $cargo) {
    Write-Host 'skip: cargo not found'
  } else {
    Push-Location (Join-Path $root 'core-rs')
    try { & $cargo test --quiet } finally { Pop-Location }
  }
}

Step 'core syntax' {
  & $py -m compileall -q (Join-Path $root 'core\exmt_core')
}

Step 'core smoke' {
  Push-Location (Join-Path $root 'core')
  try {
    $bundle = 'C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig'
    if (Test-Path $bundle) {
      $out = & $py -m exmt_core meshes $bundle
      if ($LASTEXITCODE -ne 0) { throw "meshes failed" }
      if (-not ($out -match 'c_Vepley')) { throw "unexpected meshes output" }
    } else {
      Write-Host "skip: sample bundle missing"
    }
  } finally { Pop-Location }
}

Step 'core tests' {
  # Contract tests for the self-hosted readers; no third-party packages needed.
  Push-Location (Join-Path $root 'core')
  try { & $py -m unittest discover -s tests -q } finally { Pop-Location }
}

Step 'docs build' {
  $doc = 'C:\Dev\EXMTDoc'
  if (Test-Path $doc) {
    Push-Location $doc
    try { & npx --no-install vitepress build docs } finally { Pop-Location }
  } else {
    Write-Host 'skip: C:\Dev\EXMTDoc not present'
  }
}

Write-Host "`nall checks passed" -ForegroundColor Green
