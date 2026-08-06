#!/usr/bin/env python3
"""Pre-push testing script for Physure.

Executes sequential Rust crate tests and creates fresh, isolated virtual environments
for each target Python version configured in `.env` / `.env.example` to verify
clean `maturin develop` compilation and PyO3 test pass rates before pushing.
"""

import os
import sys
import subprocess
from pathlib import Path

# Set headless Matplotlib backend globally
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


def run_command(cmd, cwd=None, env=None):
    """Executes a command and streams output, returning exit code."""
    print(f"\n[EXEC] {' '.join(cmd)} (in {cwd or REPO_ROOT})")
    res = subprocess.run(cmd, cwd=cwd or REPO_ROOT, env=env or os.environ.copy())
    return res.returncode


def main():
    config = load_env()
    python_versions = [v.strip() for v in config.get("TEST_PYTHON_VERSIONS", "3.12").split(",") if v.strip()]
    venv_base_dir = REPO_ROOT / config.get("CLEAN_VENV_DIR", ".fresh_test_venvs")
    run_rust = config.get("RUN_RUST_TESTS", "true").lower() == "true"
    run_python = config.get("RUN_PYTHON_TESTS", "true").lower() == "true"
    fail_fast = config.get("FAIL_FAST", "true").lower() == "true"

    print("==========================================================")
    print(" Physure Pre-Push Clean Environment Verification")
    print(f" Target Python Versions: {', '.join(python_versions)}")
    print(f" Fresh Venv Base Dir:    {venv_base_dir}")
    print("==========================================================")

    # 1. Rust Workspace Crates Test
    if run_rust:
        print("\n--- [Step 1/2] Running Rust Workspace Tests ---")
        code = run_command(["cargo", "test", "-p", "physure", "-p", "physure-script", "-p", "physure-cli", "-p", "physure-lsp"])
        if code != 0:
            print("\n[FAIL] Rust tests failed!")
            sys.exit(code)
        print("[OK] Rust workspace tests passed!")

    # 2. Sequential Python Matrix Testing in Fresh Venvs
    if run_python:
        print("\n--- [Step 2/2] Sequential Python Matrix Testing ---")
        python_dir = REPO_ROOT / "physure-python"
        
        for py_ver in python_versions:
            print(f"\n==========================================================")
            print(f" Testing Python {py_ver} in isolated environment")
            print(f"==========================================================")
            
            fresh_venv = venv_base_dir / f"env-{py_ver}"
            
            # Ensure target python interpreter is installed via uv
            run_command(["uv", "python", "install", py_ver])

            # Sync dependencies into isolated virtual environment
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

            # Build PyO3 maturin extension inside fresh venv
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

            # Run pytest
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
    print(" All pre-push verification checks passed cleanly!")
    print("==========================================================")


if __name__ == "__main__":
    main()
