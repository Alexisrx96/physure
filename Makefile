.PHONY: sonar test lint format help

help:
	@echo "Available commands:"
	@echo "  make sonar    - Run pytest coverage, Rust lcov, and submit SonarQube analysis"
	@echo "  make test     - Run Python and Rust test suites"
	@echo "  make lint     - Run ruff check and ruff format check"
	@echo "  make format   - Auto-format code using ruff"

sonar:
	python3 scripts/run_sonar.py

# The Python project (pyproject.toml, .venv) lives in physure-python/.
test:
	cd physure-python && uv run pytest
	cargo test --workspace

lint:
	cd physure-python && uv run ruff check .
	cd physure-python && uv run ruff format --check .

format:
	cd physure-python && uv run ruff format .
