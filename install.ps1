$ErrorActionPreference = "Stop"

$CrateName = "ralph"
$RepoUrl = "https://github.com/madhavajay/ralph"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo is required to install ralph. Install Rust from https://rustup.rs and re-run this script."
    exit 1
}

Write-Host "Installing $CrateName..."

& cargo install --locked $CrateName
if ($LASTEXITCODE -eq 0) {
    Write-Host "OK: installed via crates.io"
    exit 0
}

Write-Host "cargo install $CrateName failed, trying GitHub source..."
& cargo install --locked --git $RepoUrl $CrateName
if ($LASTEXITCODE -eq 0) {
    Write-Host "OK: installed from GitHub source"
    exit 0
}

Write-Error "Install failed."
exit 1
