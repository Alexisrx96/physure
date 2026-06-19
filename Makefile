.PHONY: sonar

CARGO := RUSTUP_HOME=/home/irvint/.cache/puccinialin/rustup CARGO_HOME=/home/irvint/.cache/puccinialin/cargo /home/irvint/.cache/puccinialin/cargo/bin/cargo

sonar:
	-uv run pytest tests/ --cov=measurekit --cov-report=xml --junitxml=test-results.xml -q
	cd measurekit_core && $(CARGO) llvm-cov --lcov --output-path lcov.info
	pysonar \
		--sonar-host-url=http://localhost:9000 \
		--sonar-token=$$(grep SONAR_TOKEN .env | cut -d= -f2) \
		--sonar-project-key=measurekit
