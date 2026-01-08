$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# Find cargo - check PATH first, then common install locations
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) {
    $cargo = $cargo.Source
} else {
    $searchPaths = @(
        "$env:USERPROFILE\.cargo\bin\cargo.exe",
        "C:\Program Files\Rust stable MSVC 1.92\bin\cargo.exe"
    )
    foreach ($path in $searchPaths) {
        if (Test-Path $path) {
            $cargo = $path
            break
        }
    }
    if (-not $cargo) {
        Write-Error "cargo not found. Install Rust from https://rustup.rs"
        exit 1
    }
}

& $cargo run --manifest-path "$ScriptDir\Cargo.toml" -- @args
