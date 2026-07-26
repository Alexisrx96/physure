# phs (PhysureScript CLI) Installer for Windows PowerShell
# Usage: irm https://physure.irvintorres.com/install.ps1 | iex
# Install from a specific branch instead of the latest release:
#   $env:PHS_BRANCH = "main"; irm https://physure.irvintorres.com/install.ps1 | iex

$ErrorActionPreference = 'Stop'
$Repo = "Alexisrx96/physure"
$InstallDir = "$HOME\.local\bin"
$Branch = $env:PHS_BRANCH

Write-Host "⚡ Installing phs (PhysureScript CLI)..." -ForegroundColor Cyan

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

function Install-FromSource {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error "Rust/cargo not found. Install it from https://rustup.rs then re-run this script."
        exit 1
    }
    if ($Branch) {
        Write-Host "Building from source (branch: $Branch)..."
        cargo install --git "https://github.com/$Repo" --branch $Branch physure-cli --bin phs --locked --force
    } else {
        Write-Host "Building from source (default branch)..."
        cargo install --git "https://github.com/$Repo" physure-cli --bin phs --locked --force
    }
    Copy-Item "$HOME\.cargo\bin\phs.exe" "$InstallDir\phs.exe" -Force

    try {
        if ($Branch) {
            cargo install --git "https://github.com/$Repo" --branch $Branch physure-lsp --locked --force
        } else {
            cargo install --git "https://github.com/$Repo" physure-lsp --locked --force
        }
        Copy-Item "$HOME\.cargo\bin\physure-lsp.exe" "$InstallDir\physure-lsp.exe" -Force
    } catch {
        Write-Host "Warning: failed to build physure-lsp (VS Code language server); continuing without it." -ForegroundColor Yellow
    }
}

$installed = $false
if (-not $Branch) {
    try {
        $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases"
        $release = $releases | Where-Object { $_.tag_name -like "core-v*" } | Select-Object -First 1
        if ($release) {
            $asset = $release.assets | Where-Object { $_.name -eq "phs-windows-x86_64.zip" } | Select-Object -First 1
            if ($asset) {
                $zipPath = Join-Path $env:TEMP "phs-windows-x86_64.zip"
                Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath
                Expand-Archive -Path $zipPath -DestinationPath $InstallDir -Force
                Remove-Item $zipPath
                $installed = $true
            }
        }
    } catch {
        $installed = $false
    }
    if (-not $installed) {
        Write-Host "No prebuilt binary found — falling back to cargo."
    }
}

if (-not $installed) {
    Install-FromSource
}

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "✨ Added $InstallDir to User PATH environment variable." -ForegroundColor Green
}

Write-Host "`n🎉 phs successfully installed!" -ForegroundColor Green
Write-Host "Try running: phs  or  phs `"500 N / 2 m^2 => kPa`"" -ForegroundColor Cyan
