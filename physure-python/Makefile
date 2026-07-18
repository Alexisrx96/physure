.PHONY: sonar

CARGO_BIN := /home/irvint/.cache/puccinialin/cargo/bin
CARGO := RUSTUP_HOME=/home/irvint/.cache/puccinialin/rustup CARGO_HOME=/home/irvint/.cache/puccinialin/cargo $(CARGO_BIN)/cargo
export PATH := $(CARGO_BIN):$(PATH)

# pyo3's extension-module feature skips linking libpython, so the Rust test
# binary needs explicit link flags + a Python that can import measurekit/numpy.
# Derive everything from the project venv (machine-independent).
PY := $(CURDIR)/.venv/bin/python
PYLIBDIR := $(shell $(PY) -c "import sysconfig; print(sysconfig.get_config_var('LIBDIR'))")
PYLDVER := $(shell $(PY) -c "import sysconfig; print(sysconfig.get_config_var('LDVERSION'))")
PYSITE := $(shell $(PY) -c "import site; print(site.getsitepackages()[0])")
PYENV := RUSTFLAGS="-L$(PYLIBDIR) -lpython$(PYLDVER)" LD_LIBRARY_PATH="$(PYLIBDIR)" PYTHONPATH="$(PYSITE):$(CURDIR)"

sonar:
	-uv run pytest tests/ --cov=physure --cov-report=xml --junitxml=test-results.xml -q
	cd ../physure-core && $(PYENV) $(CARGO) llvm-cov --lcov --output-path lcov.info
	python3 ../physure-core/scripts/filter_lcov.py ../physure-core/lcov.info
	pysonar \
		--sonar-host-url=http://sonar.localhost \
		--sonar-token=$$(grep '^SONAR_TOKEN=' ../.env | cut -d= -f2) \
		--sonar-project-key=physure
	pysonar \
		--sonar-host-url=http://sonar.localhost \
		--sonar-token=$$(grep '^SONAR_TOKEN_CORE=' ../.env | cut -d= -f2) \
		--sonar-project-key=physure-core \
		--sonar-project-base-dir=../physure-core
