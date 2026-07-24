@echo off
rem phs (PhysureScript CLI) Installer for Windows Command Prompt
rem Usage: install.cmd  (or: curl -fsSL <url>/install.cmd -o install.cmd ^&^& install.cmd)

setlocal

echo Installing phs (PhysureScript CLI)...

where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust/cargo not found. Install it from https://rustup.rs then re-run this script.
    exit /b 1
)

rem physure-cli (the phs binary) isn't published to crates.io, so install
rem straight from the repo.
cargo install --git https://github.com/Alexisrx96/physure physure-cli --bin phs --locked --force
if errorlevel 1 (
    echo cargo install failed.
    exit /b 1
)

echo.
echo phs installed to %USERPROFILE%\.cargo\bin\phs.exe

echo %PATH% | find /I "%USERPROFILE%\.cargo\bin" >nul
if errorlevel 1 (
    echo NOTE: %USERPROFILE%\.cargo\bin is not on your PATH.
    echo Rustup normally adds it for you; if not, add it via System Properties ^> Environment Variables.
)

echo.
echo phs successfully installed!
echo Try: phs "500 N / 2 m^^2 =^> kPa"
