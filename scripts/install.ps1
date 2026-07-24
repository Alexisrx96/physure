# phs (PhysureScript CLI) Installer for Windows PowerShell
# Usage: irm https://physure.irvintorres.com/install.ps1 | iex

$ErrorActionPreference = 'Stop'

Write-Host "⚡ Installing phs (PhysureScript CLI)..." -ForegroundColor Cyan

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Rust/cargo not found. Install it from https://rustup.rs then re-run this script."
    exit 1
}

# physure-cli (the phs binary) isn't published to crates.io, so install
# straight from the repo.
cargo install --git https://github.com/Alexisrx96/physure physure-cli --bin phs --locked --force

$CargoBin = "$HOME\.cargo\bin"
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$CargoBin*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$CargoBin", "User")
    $env:Path = "$env:Path;$CargoBin"
    Write-Host "✨ Added $CargoBin to User PATH environment variable." -ForegroundColor Green
}

Write-Host "`n🎉 phs successfully installed!" -ForegroundColor Green
Write-Host "Try running: phs  or  phs `"500 N / 2 m^2 => kPa`"" -ForegroundColor Cyan
