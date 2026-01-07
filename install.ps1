$ErrorActionPreference = "Stop"

$Repo = "madhavajay/ralph"
$BinName = "ralph"

function Fail {
    param([string]$Message)
    Write-Error $Message
    Write-Error "If no prebuilt binary is available, use: cargo install ralph"
    exit 1
}

$Arch = $env:PROCESSOR_ARCHITECTURE
switch ($Arch) {
    "AMD64" { $Arch = "x86_64" }
    "ARM64" { $Arch = "aarch64" }
    default { Fail "unsupported arch: $Arch" }
}

$Target = "$Arch-pc-windows-msvc"
$Asset = "$BinName-$Target.zip"
$Url = "https://github.com/$Repo/releases/latest/download/$Asset"

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
$ZipPath = Join-Path $TempDir $Asset

try {
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath
} catch {
    Fail "failed to download $Url"
}

try {
    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force
} catch {
    Fail "failed to extract $ZipPath"
}

$BinPath = Get-ChildItem -Path $TempDir -Recurse -Filter "$BinName.exe" | Select-Object -First 1
if (-not $BinPath) {
    Fail "download succeeded but $BinName.exe not found in archive"
}

$BaseDir = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { $env:USERPROFILE }
$InstallDir = Join-Path $BaseDir "ralph\bin"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force $BinPath.FullName (Join-Path $InstallDir "$BinName.exe")

Write-Host "Installed $BinName to $InstallDir\$BinName.exe"
if (($env:Path -split ';') -notcontains $InstallDir) {
    Write-Host "Note: $InstallDir is not on your PATH. Add it to use '$BinName' directly."
}

Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
