# Local web build for Windows.
# Produces dist/web/multifile (index.html + app.js + wasm) and
# dist/web/single (one self-contained html).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root

rustup target add wasm32-unknown-unknown | Out-Host

cargo rustc --release --target wasm32-unknown-unknown --bin re-ch-rzl-rust -- -C link-arg=--allow-undefined
if (-not $?) {
    throw "wasm build failed"
}

$wasm = Join-Path $root "target\wasm32-unknown-unknown\release\re-ch-rzl-rust.wasm"
if (-not (Test-Path -LiteralPath $wasm)) {
    throw "wasm not found: $wasm"
}

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    $python = Get-Command python3 -ErrorAction SilentlyContinue
}
if (-not $python) {
    throw "python not found"
}

& $python.Source (Join-Path $root ".github\scripts\build_web.py") --wasm $wasm --out (Join-Path $root "dist\web")
if (-not $?) {
    throw "web packaging failed"
}

Write-Host "multifile: dist\web\multifile\index.html"
Write-Host "single:    dist\web\single\re-ch-rzl-rust.html"
