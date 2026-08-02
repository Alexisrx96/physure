.PHONY: sonar test lint format

sonar:
	python3 ../scripts/run_sonar.py

test:
	uv run pytest

lint:
	uv run ruff check .
	uv run ruff format --check .

format:
	uv run ruff format .
