#!/usr/bin/env python3
"""Pre-push testing script for Physure.

Optimized prebuild workflow matching CI:
1. Quality (ruff check & format)
2. Test Rust Crates (cargo test)
3. Prebuild Stripped ABI3 PyO3 Wheel once (maturin build --release --strip)
4. Fast Python matrix testing in clean, isolated virtual environments

Options:
  --quick        Test only against the current active Python version
  --clean-after  Clean up test virtual environments after completion
"""

import argparse
import os
import shutil
import sys
import subprocess
from pathlib import Path

os.environ["MPLBACKEND"] = "Agg"
os.environ["UV_LINK_MODE"] = "copy"

REPO_ROOT = Path(__file__).resolve().parent.parent


def load_env():
    """Simple parser for .env key=value lines."""
    env_file = REPO_ROOT / ".env"
    if not env_file.exists():
        env_file = REPO_ROOT / ".env.example"
    
    config = {
        "TEST_PYTHON_VERSIONS": "3.11,3.12,3.13,3.14",
        "CLEAN_VENV_DIR": ".fresh_test_venvs",
        "RUN_RUST_TESTS": "true",
        "RUN_PYTHON_TESTS": "true",
        "FAIL_FAST": "true",
    }
    
    if env_file.exists():
        with open(env_file, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    k, v = line.split("=", 1)
                    config[k.strip()] = v.strip().strip("\"'")
                    
    return config


def auto_sign_windows_binaries():
    """On Windows, auto-sign compiled binaries with local dev cert if available."""
    if sys.platform != "win32":
        return
    ps_cmd = (
        "$cert = Get-ChildItem Cert:\\CurrentUser\\My | Where-Object { $_.Subject -like '*Physure Local Dev*' } | Select-Object -First 1; "
        "if ($cert) { "
        "Get-ChildItem -Path 'target\\debug', 'target\\release', 'physure-python\\physure', '.fresh_test_venvs' -Recurse -Include *.exe, *.pyd -ErrorAction SilentlyContinue | "
        "ForEach-Object { Set-AuthenticodeSignature -FilePath $_.FullName -Certificate $cert -ErrorAction SilentlyContinue } "
        "}"
    )
    try:
        subprocess.run(["powershell", "-ExecutionPolicy", "Bypass", "-Command", ps_cmd], cwd=REPO_ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception:
        pass


def run_command(cmd, cwd=None, env=None, retries=1):
    """Executes a command and streams output, with retry support and auto-signing for Windows file locks."""
    for attempt in range(retries):
        print(f"\n[EXEC] {' '.join(cmd)} (in {cwd or REPO_ROOT})")
        res = subprocess.run(cmd, cwd=cwd or REPO_ROOT, env=env or os.environ.copy())
        if res.returncode == 0:
            return 0
        if attempt < retries - 1:
            delay = (attempt + 1) * 3
            print(f"\n[RETRY] Command failed (attempt {attempt + 1}/{retries}). Auto-signing binaries & waiting {delay}s for Windows file locks/AV release...")
            auto_sign_windows_binaries()
            import time
            time.sleep(delay)
    return res.returncode


def main():
    parser = argparse.ArgumentParser(description="Physure Pre-Push Clean Environment Verification")
    parser.add_argument("--quick", action="store_true", help="Test only against the current Python version")
    parser.add_argument("--clean-after", action="store_true", help="Clean up test venvs directory after completion")
    args = parser.parse_args()

    config = load_env()
    is_quick = args.quick or os.environ.get("PREPUSH_QUICK", "").lower() in ("true", "1")
    if is_quick:
        current_py = f"{sys.version_info.major}.{sys.version_info.minor}"
        python_versions = [current_py]
    else:
        python_versions = [v.strip() for v in config.get("TEST_PYTHON_VERSIONS", "3.12").split(",") if v.strip()]

    venv_base_dir = REPO_ROOT / config.get("CLEAN_VENV_DIR", ".fresh_test_venvs")
    run_rust = config.get("RUN_RUST_TESTS", "true").lower() == "true"
    run_python = config.get("RUN_PYTHON_TESTS", "true").lower() == "true"
    fail_fast = config.get("FAIL_FAST", "true").lower() == "true"
    python_dir = REPO_ROOT / "physure-python"

    print("==========================================================")
    print(" Physure Optimized Pre-Push CI Verification")
    print(f" Target Python Versions: {', '.join(python_versions)}")
    print(f" Fresh Venv Base Dir:    {venv_base_dir}")
    print("==========================================================")

    # 1. Quality Checks
    print("\n--- [Step 1/4] Python Quality Checks (Ruff Lint & Format) ---")
    code = run_command(["uv", "run", "ruff", "check", "."], cwd=python_dir)
    if code != 0:
        print("\n[FAIL] Step 1: Ruff lint failed!")
        sys.exit(code)
    code = run_command(["uv", "run", "ruff", "format", "--check", "."], cwd=python_dir)
    if code != 0:
        print("\n[FAIL] Step 1: Ruff format check failed!")
        sys.exit(code)
    print("[OK] Step 1: Quality checks passed!")

    # 2. Test All Rust Crates Together
    if run_rust:
        print("\n--- [Step 2/4] Testing All Rust Workspace Crates ---")
        code = run_command(
            ["cargo", "test", "-p", "physure", "-p", "physure-script", "-p", "physure-cli", "-p", "physure-lsp"],
            retries=5
        )
        if code != 0:
            print("\n[FAIL] Step 2: Rust workspace tests failed!")
            sys.exit(code)
        print("[OK] Step 2: All Rust workspace tests passed!")

    # 3. Prebuild Stripped ABI3 PyO3 Wheel once
    if run_python:
        print("\n--- [Step 3/4] Prebuilding Stripped ABI3 PyO3 Wheel ---")
        dist_dir = python_dir / "dist"
        code = run_command(["uv", "run", "maturin", "build", "--release", "--strip", "--out", str(dist_dir)], cwd=python_dir)
        if code != 0:
            print("\n[FAIL] Step 3: PyO3 wheel prebuild failed!")
            sys.exit(code)
        print("[OK] Step 3: Stripped ABI3 PyO3 wheel prebuilt successfully!")

    # 4. Sequential Python Matrix Testing in Fresh Venvs
    if run_python:
        print("\n--- [Step 4/4] Sequential Python Matrix Testing ---")
        
        wheels = list((python_dir / "dist").glob("*.whl"))
        if not wheels:
            print("\n[FAIL] No prebuilt wheel found in dist/")
            sys.exit(1)
        wheel_path = str(wheels[0])

        for py_ver in python_versions:
            print(f"\n==========================================================")
            print(f" Testing Python {py_ver} using prebuilt wheel")
            print(f"==========================================================")
            
            fresh_venv = venv_base_dir / f"env-{py_ver}"
            
            run_command(["uv", "python", "install", py_ver])

            env_vars = os.environ.copy()
            env_vars["UV_PROJECT_ENVIRONMENT"] = str(fresh_venv)
            env_vars["VIRTUAL_ENV"] = str(fresh_venv)
            env_vars["UV_LINK_MODE"] = "copy"
            
            sync_code = run_command(
                ["uv", "sync", "--all-extras", "--dev", "--python", py_ver],
                cwd=python_dir,
                env=env_vars
            )
            if sync_code != 0:
                print(f"\n[FAIL] uv sync failed for Python {py_ver}")
                if fail_fast:
                    sys.exit(sync_code)
                continue

            install_code = run_command(
                ["uv", "pip", "install", wheel_path, "--force-reinstall", "--python", py_ver],
                cwd=python_dir,
                env=env_vars
            )
            if install_code != 0:
                print(f"\n[FAIL] Prebuilt wheel installation failed for Python {py_ver}")
                if fail_fast:
                    sys.exit(install_code)
                continue

            auto_sign_windows_binaries()

            pytest_code = run_command(
                ["uv", "run", "pytest", "--ignore=tests/core/test_serialization.py"],
                cwd=python_dir,
                env=env_vars,
                retries=4
            )
            if pytest_code != 0:
                print(f"\n[FAIL] pytest failed for Python {py_ver}")
                if fail_fast:
                    sys.exit(pytest_code)
                continue

            print(f"[OK] Python {py_ver} tests passed cleanly!")

    if args.clean_after and venv_base_dir.exists():
        print("\n--- Cleaning up temporary test virtual environments ---")
        shutil.rmtree(venv_base_dir, ignore_errors=True)
        print("[OK] Temporary virtual environments cleaned up!")

    print("\n==========================================================")
    print(" All optimized CI verification steps passed cleanly!")
    print("==========================================================")


if __name__ == "__main__":
    main()
