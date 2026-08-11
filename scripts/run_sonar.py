#!/usr/bin/env python3
"""
SonarQube Runner for Physure.
Runs Python & Rust coverage generation and submits analysis to SonarQube.
"""

import os
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

# Paths
REPO_ROOT = Path(__file__).resolve().parent.parent
PHYSURE_CORE = REPO_ROOT / "physure-core"
PHYSURE_PYTHON = REPO_ROOT / "physure-python"
ENV_FILE = REPO_ROOT / ".env"

# Colors for friendly terminal output
GREEN = "\033[92m"
YELLOW = "\033[93m"
RED = "\033[91m"
BLUE = "\033[94m"
BOLD = "\033[1m"
RESET = "\033[0m"

def log_info(msg: str):
    print(f"{BLUE}{BOLD}[INFO]{RESET} {msg}")

def log_success(msg: str):
    print(f"{GREEN}{BOLD}[SUCCESS]{RESET} {msg}")

def log_warning(msg: str):
    print(f"{YELLOW}{BOLD}[WARNING]{RESET} {msg}")

def log_error(msg: str):
    print(f"{RED}{BOLD}[ERROR]{RESET} {msg}")


def load_env_file():
    """Load environment variables from .env if present."""
    if not ENV_FILE.exists():
        return
    log_info(f"Loading environment variables from {ENV_FILE.name}")
    with open(ENV_FILE, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, val = line.split("=", 1)
            key = key.strip()
            val = val.strip().strip("'").strip('"')
            if key and not os.getenv(key):
                os.environ[key] = val


def check_sonar_server(host_url: str) -> bool:
    """Check if SonarQube server is reachable and UP."""
    status_url = f"{host_url.rstrip('/')}/api/system/status"
    try:
        req = urllib.request.Request(status_url, headers={"User-Agent": "physure-sonar-check"})
        with urllib.request.urlopen(req, timeout=3) as resp:
            if resp.status == 200:
                data = resp.read().decode("utf-8")
                if '"status":"UP"' in data or "UP" in data:
                    return True
    except Exception as e:
        pass

    # Fallback attempt for root URL
    try:
        req = urllib.request.Request(host_url, headers={"User-Agent": "physure-sonar-check"})
        with urllib.request.urlopen(req, timeout=3) as resp:
            if resp.status in (200, 302, 401):
                return True
    except Exception:
        pass

    return False


def find_cargo_bin() -> str:
    """Find cargo executable, searching standard locations including local caches."""
    cargo = shutil.which("cargo")
    if cargo:
        return cargo
    
    candidates = [
        Path.home() / ".cargo" / "bin" / "cargo",
        Path.home() / ".cache" / "puccinialin" / "cargo" / "bin" / "cargo",
    ]
    for candidate in candidates:
        if candidate.exists() and os.access(candidate, os.X_OK):
            return str(candidate)
    return "cargo"


def find_llvm_cov_env() -> tuple[dict, str | None]:
    """Build environment variables needed for cargo llvm-cov."""
    env = os.environ.copy()
    
    # Check if cargo-llvm-cov exists in PATH or candidate dirs
    cargo_bin_dirs = [
        Path.home() / ".cache" / "puccinialin" / "cargo" / "bin",
        Path.home() / ".cargo" / "bin",
    ]
    
    llvm_cov_found = False
    for bin_dir in cargo_bin_dirs:
        llvm_cov_bin = bin_dir / "cargo-llvm-cov"
        if llvm_cov_bin.exists():
            env["PATH"] = f"{bin_dir}:{env.get('PATH', '')}"
            llvm_cov_found = True
            break

    if not llvm_cov_found and shutil.which("cargo-llvm-cov"):
        llvm_cov_found = True

    # Configure Python linkage for PyO3 Rust tests
    python_bin = sys.executable
    try:
        import sysconfig
        pylibdir = sysconfig.get_config_var('LIBDIR') or ''
        pyldver = sysconfig.get_config_var('LDVERSION') or ''
        if pylibdir and pyldver:
            env["RUSTFLAGS"] = f"-L{pylibdir} -lpython{pyldver}"
            env["LD_LIBRARY_PATH"] = f"{pylibdir}:{env.get('LD_LIBRARY_PATH', '')}"
    except Exception:
        pass

    return env, ("cargo-llvm-cov" if llvm_cov_found else None)


def compile_physure_java():
    """Compile physure-java so the Java sensor has class files (sonar.java.binaries)."""
    mvn_bin = shutil.which("mvn")
    if not mvn_bin:
        log_warning("mvn not found. Skipping physure-java compile (Java analysis will fail without target/classes).")
        return

    log_info("Compiling physure-java for SonarQube Java analysis...")
    try:
        subprocess.run([mvn_bin, "-q", "compile"], cwd=REPO_ROOT / "physure-java", check=True)
        log_success("physure-java compiled successfully (target/classes)")
    except Exception as e:
        log_warning(f"physure-java compile failed: {e}")


def main():
    load_env_file()

    # Determine Sonar host URL
    host_url = os.getenv("SONAR_HOST_URL", "").strip()
    if not host_url:
        # Check standard defaults
        if check_sonar_server("http://sonar.localhost"):
            host_url = "http://sonar.localhost"
        elif check_sonar_server("http://localhost:9000"):
            host_url = "http://localhost:9000"
        else:
            host_url = "http://sonar.localhost"

    log_info(f"Target SonarQube Server: {host_url}")

    # Check server status
    if not check_sonar_server(host_url):
        log_warning(f"Could not connect to SonarQube server at {host_url}")
        log_info("To start a local SonarQube instance, run:")
        print(f"   docker run -d --name sonarqube -p 9000:9000 sonarqube:lts\n")
        log_info("Continuing with test & coverage generation steps...")

    sonar_token = os.getenv("SONAR_TOKEN", "")
    sonar_token_core = os.getenv("SONAR_TOKEN_CORE", sonar_token)

    # 1. Run Python Coverage
    log_info("Running Python test suite and coverage...")
    pytest_cmd = ["uv", "run", "pytest", "tests/", "--cov=physure", "--cov-report=xml", "--junitxml=test-results.xml", "-q"]
    try:
        subprocess.run(pytest_cmd, cwd=PHYSURE_PYTHON, check=True)
        log_success("Python coverage report generated successfully (coverage.xml)")
    except Exception as e:
        log_warning(f"Python pytest returned errors or non-zero exit: {e}")

    # 2. Run Rust Coverage (if cargo-llvm-cov is available)
    env, llvm_cov_bin = find_llvm_cov_env()
    if llvm_cov_bin:
        log_info("Generating Rust LCOV coverage via cargo-llvm-cov...")
        cargo_bin = find_cargo_bin()
        cov_cmd = [cargo_bin, "llvm-cov", "--auto-install", "--lcov", "--output-path", "lcov.info"]
        try:
            subprocess.run(cov_cmd, cwd=PHYSURE_CORE, env=env, check=True)
            filter_script = PHYSURE_CORE / "scripts" / "filter_lcov.py"
            if filter_script.exists():
                subprocess.run([sys.executable, str(filter_script), str(PHYSURE_CORE / "lcov.info")], check=False)
            log_success("Rust coverage report generated successfully (lcov.info)")
        except Exception as e:
            log_warning(f"Rust coverage generation failed: {e}")
    else:
        log_warning("cargo-llvm-cov not found. Skipping Rust lcov generation (Python coverage will still be submitted).")

    # 2b. Compile physure-java so the Java sensor has class files (sonar.java.binaries)
    compile_physure_java()

    # 3. Submit to SonarQube using pysonar or sonar-scanner
    pysonar_bin = shutil.which("pysonar")
    if not pysonar_bin:
        local_pysonar = Path.home() / ".local" / "bin" / "pysonar"
        if local_pysonar.exists():
            pysonar_bin = str(local_pysonar)
        else:
            pysonar_bin = shutil.which("sonar-scanner") or "pysonar"

    subprojects = [
        ("physure-python", "physure", "SONAR_TOKEN"),
        ("physure-core", "physure-core", "SONAR_TOKEN_CORE"),
        ("physure-java", "physure-java", "SONAR_TOKEN_JAVA"),
        ("physure-cli", "physure-cli", "SONAR_TOKEN_CLI"),
        ("physure-lsp", "physure-lsp", "SONAR_TOKEN_LSP"),
        ("physure-script", "physure-script", "SONAR_TOKEN_SCRIPT"),
        ("physure-wasm", "physure-wasm", "SONAR_TOKEN_WASM"),
    ]

    for dir_name, key, token_var in subprojects:
        subpath = REPO_ROOT / dir_name
        props = subpath / "sonar-project.properties"
        if not props.exists():
            continue

        log_info(f"Submitting {key} ({dir_name}) analysis to SonarQube...")
        token = os.getenv(token_var, sonar_token)
        cmd = [
            str(pysonar_bin),
            f"--sonar-host-url={host_url}",
            f"--sonar-project-key={key}",
        ]
        if dir_name != "physure-python":
            cmd.append(f"--sonar-project-base-dir={subpath}")

        if token:
            cmd.append(f"--sonar-token={token}")

        try:
            subprocess.run(cmd, cwd=PHYSURE_PYTHON, check=True)
            log_success(f"{key} analysis submitted successfully!")
        except Exception as e:
            log_warning(f"Failed to submit {key} analysis: {e}")

    log_success(f"SonarQube workflow complete! View dashboard at: {host_url}")


if __name__ == "__main__":
    main()
