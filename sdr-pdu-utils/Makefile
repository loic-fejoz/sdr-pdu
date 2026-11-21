CARGO ?= cargo
DOCKER ?= docker
REPO_ROOT := $(shell git rev-parse --show-toplevel 2>/dev/null || pwd/..)

.PHONY: test test-rust test-gnuradio build-gnuradio-image

test: test-rust test-gnuradio

test-rust:
	$(CARGO) test

build-gnuradio-image:
	@if [ -z "$$($(DOCKER) images -q gnuradio-test 2> /dev/null)" ]; then \
		echo "Building gnuradio-test Docker image..."; \
		$(DOCKER) buildx build --load -t gnuradio-test $(REPO_ROOT)/docker/gnuradio; \
	fi

test-gnuradio: build-gnuradio-image
	@echo "Generating test signals for GNU Radio..."
	$(CARGO) run --example generate_test_signals
	@echo "Running GNU Radio verification inside Docker..."
	$(DOCKER) run --rm -v "$(REPO_ROOT)/docker/gnuradio/scripts:/workspace/scripts" gnuradio-test python3 /workspace/scripts/test_gnuradio.py
