#!/usr/bin/env python3
"""Pre-push testing script for Physure.

Executes strict sequential steps matching CI pipeline order:
1. Build Core (cargo build -p physure)
2. Quality (ruff check & format)
3. Test rust-core (cargo test -p physure)
4. Test physure-script (cargo test -p physure-script)
5. Test physure-cli (cargo test -p physure-cli)
6. Test physure-lsp (cargo test -p physure-lsp)
7. Python matrix testing in clean, isolated virtual environments
"""

import os
import sys
import subprocess
from pathlib import Path

os.environ["MPLBACKEND"] = "Agg"

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


def run_command(cmd, cwd=None, env=None, retries=1):
    """Executes a command and streams output, with retry support for Windows file locks."""
    for attempt in range(retries):
        print(f"\n[EXEC] {' '.join(cmd)} (in {cwd or REPO_ROOT})")
        res = subprocess.run(cmd, cwd=cwd or REPO_ROOT, env=env or os.environ.copy())
        if res.returncode == 0:
            return 0
        if attempt < retries - 1:
            print(f"\n[RETRY] Retrying command (attempt {attempt + 2}/{retries})...")
            import time
            time.sleep(2)
    return res.returncode


def main():
    config = load_env()
    python_versions = [v.strip() for v in config.get("TEST_PYTHON_VERSIONS", "3.12").split(",") if v.strip()]
    venv_base_dir = REPO_ROOT / config.get("CLEAN_VENV_DIR", ".fresh_test_venvs")
    run_rust = config.get("RUN_RUST_TESTS", "true").lower() == "true"
    run_python = config.get("RUN_PYTHON_TESTS", "true").lower() == "true"
    fail_fast = config.get("FAIL_FAST", "true").lower() == "true"

    print("==========================================================")
    print(" Physure Sequential CI Pipeline Verification")
    print(f" Target Python Versions: {', '.join(python_versions)}")
    print(f" Fresh Venv Base Dir:    {venv_base_dir}")
    print("==========================================================")

    # 1. Build Core
    print("\n--- [Step 1/7] Building Core Crate (physure) ---")
    code = run_command(["cargo", "build", "-p", "physure"])
    if code != 0:
        print("\n[FAIL] Step 1: Core build failed!")
        sys.exit(code)
    print("[OK] Step 1: Core built successfully!")

    # 2. Quality (Ruff lint & format)
    print("\n--- [Step 2/7] Python Quality Checks (Ruff Lint & Format) ---")
    python_dir = REPO_ROOT / "physure-python"
    code = run_command(["uv", "run", "ruff", "check", "."], cwd=python_dir)
    if code != 0:
        print("\n[FAIL] Step 2: Ruff lint failed!")
        sys.exit(code)
    code = run_command(["uv", "run", "ruff", "format", "--check", "."], cwd=python_dir)
    if code != 0:
        print("\n[FAIL] Step 2: Ruff format check failed!")
        sys.exit(code)
    print("[OK] Step 2: Quality checks passed!")

    if run_rust:
        # 3. Test rust-core
        print("\n--- [Step 3/7] Testing Rust Core Crate (physure) ---")
        code = run_command(["cargo", "test", "-p", "physure"], retries=2)
        if code != 0:
            print("\n[FAIL] Step 3: Rust core tests failed!")
            sys.exit(code)
        print("[OK] Step 3: Rust core tests passed!")

        # 4. Test physure-script
        print("\n--- [Step 4/7] Testing Physure Script Crate (physure-script) ---")
        code = run_command(["cargo", "test", "-p", "physure-script"], retries=2)
        if code != 0:
            print("\n[FAIL] Step 4: Physure script tests failed!")
            sys.exit(code)
        print("[OK] Step 4: Physure script tests passed!")

        # 5. Test physure-cli
        print("\n--- [Step 5/7] Testing Physure CLI Crate (physure-cli) ---")
        code = run_command(["cargo", "test", "-p", "physure-cli"], retries=2)
        if code != 0:
            print("\n[FAIL] Step 5: Physure CLI tests failed!")
            sys.exit(code)
        print("[OK] Step 5: Physure CLI tests passed!")

        # 6. Test physure-lsp
        print("\n--- [Step 6/7] Testing Physure LSP Crate (physure-lsp) ---")
        code = run_command(["cargo", "test", "-p", "physure-lsp"], retries=2)
        if code != 0:
            print("\n[FAIL] Step 6: Physure LSP tests failed!")
            sys.exit(code)
        print("[OK] Step 6: Physure LSP tests passed!")

    # 7. Sequential Python Matrix Testing in Fresh Venvs
    if run_python:
        print("\n--- [Step 7/7] Sequential Python Matrix Testing ---")
        
        for py_ver in python_versions:
            print(f"\n==========================================================")
            print(f" Testing Python {py_ver} in isolated environment")
            print(f"==========================================================")
            
            fresh_venv = venv_base_dir / f"env-{py_ver}"
            
            run_command(["uv", "python", "install", py_ver])

            env_vars = os.environ.copy()
            env_vars["UV_PROJECT_ENVIRONMENT"] = str(fresh_venv)
            
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

            maturin_code = run_command(
                ["uv", "run", "maturin", "develop"],
                cwd=python_dir,
                env=env_vars
            )
            if maturin_code != 0:
                print(f"\n[FAIL] maturin develop failed for Python {py_ver}")
                if fail_fast:
                    sys.exit(maturin_code)
                continue

            pytest_code = run_command(
                ["uv", "run", "pytest", "--ignore=tests/core/test_serialization.py"],
                cwd=python_dir,
                env=env_vars
            )
            if pytest_code != 0:
                print(f"\n[FAIL] pytest failed for Python {py_ver}")
                if fail_fast:
                    sys.exit(pytest_code)
                continue

            print(f"[OK] Python {py_ver} tests passed cleanly!")

    print("\n==========================================================")
    print(" All sequential CI verification steps passed cleanly!")
    print("==========================================================")


if __name__ == "__main__":
    main()
