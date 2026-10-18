# Quality gate for EXMT Modbox. Run: bun run check
# Fails fast on any step; prints a one-line summary per step.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$py = 'C:\Users\Administrator\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe'
if (-not (Test-Path $py)) { $py = 'python' }

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
  try { & npx --no-install vue-tsc --noEmit } finally { Pop-Location }
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
