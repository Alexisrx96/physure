@echo off
rem phs (PhysureScript CLI) Installer for Windows Command Prompt
rem Usage: install.cmd  (or: curl -fsSL <url>/install.cmd -o install.cmd ^&^& install.cmd)
rem Install from a specific branch instead of the latest release:
rem   set PHS_BRANCH=main ^&^& install.cmd

setlocal

echo Installing phs (PhysureScript CLI)...

set "INSTALL_DIR=%USERPROFILE%\.local\bin"
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

if defined PHS_BRANCH goto :from_source

rem Delegate the download to PowerShell (always present on Windows) rather
rem than hand-parsing JSON in batch.
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ErrorActionPreference='Stop'; try { $r = Invoke-RestMethod -Uri 'https://api.github.com/repos/Alexisrx96/physure/releases' | Where-Object { $_.tag_name -like 'core-v*' } | Select-Object -First 1; $a = $r.assets | Where-Object { $_.name -eq 'phs-windows-x86_64.zip' } | Select-Object -First 1; if (-not $a) { exit 1 }; Invoke-WebRequest -Uri $a.browser_download_url -OutFile \"$env:TEMP\phs-windows-x86_64.zip\"; Expand-Archive -Path \"$env:TEMP\phs-windows-x86_64.zip\" -DestinationPath '%INSTALL_DIR%' -Force; Remove-Item \"$env:TEMP\phs-windows-x86_64.zip\" } catch { exit 1 }"

if exist "%INSTALL_DIR%\phs.exe" goto :done
echo No prebuilt binary found - falling back to cargo.

:from_source
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust/cargo not found. Install it from https://rustup.rs then re-run this script.
    exit /b 1
)
if defined PHS_BRANCH (
    echo Building from source ^(branch: %PHS_BRANCH%^)...
    cargo install --git https://github.com/Alexisrx96/physure --branch %PHS_BRANCH% physure-cli --bin phs --locked --force
) else (
    echo Building from source ^(default branch^)...
    cargo install --git https://github.com/Alexisrx96/physure physure-cli --bin phs --locked --force
)
if errorlevel 1 (
    echo cargo install failed.
    exit /b 1
)
copy /y "%USERPROFILE%\.cargo\bin\phs.exe" "%INSTALL_DIR%\phs.exe" >nul

:done

echo.
echo phs installed to %INSTALL_DIR%\phs.exe

echo %PATH% | find /I "%INSTALL_DIR%" >nul
if errorlevel 1 (
    echo NOTE: %INSTALL_DIR% is not on your PATH.
    echo Add it via System Properties ^> Environment Variables.
)

echo.
echo phs successfully installed!
echo Try: phs "500 N / 2 m^^2 =^> kPa"
