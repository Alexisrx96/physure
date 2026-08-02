.PHONY: sonar test lint format help

help:
	@echo "Available commands:"
	@echo "  make sonar    - Run pytest coverage, Rust lcov, and submit SonarQube analysis"
	@echo "  make test     - Run Python and Rust test suites"
	@echo "  make lint     - Run ruff check and ruff format check"
	@echo "  make format   - Auto-format code using ruff"

sonar:
	python3 scripts/run_sonar.py

test:
	uv run pytest
	cargo test --workspace

lint:
	uv run ruff check .
	uv run ruff format --check .

format:
	uv run ruff format .
