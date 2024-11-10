DOCKERFILES ?= $(shell find . -maxdepth 1 -name 'Dockerfile*' -execdir basename '{}' ';')
IMAGE_NAME ?=  ghcr.io/rash-sh/rash
IMAGE_VERSION ?= latest

BOOK_DIR ?= .
CARGO_TARGET ?= x86_64-unknown-linux-gnu
CARGO_BUILD_PARAMS = --target=$(CARGO_TARGET)
# use cargo if same target or cross if not
CARGO += $(if $(filter $(shell uname -m)-unknown-linux-gnu,$(CARGO_TARGET)),cargo,cross)
ifeq ($(CARGO),cross)
	CARGO_BUILD_PARAMS +=  --target-dir $(shell pwd)/$(CARGO_TARGET_DIR)
endif
PKG_BASE_NAME ?= rash-${CARGO_TARGET}
VERSION ?= master

CARGO_TARGET_DIR ?= target

PROJECT_VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)

.DEFAULT: help
.PHONY: help
help:	## show this help menu.
	@echo "Usage: make [TARGET ...]"
	@echo ""
	@@egrep -h "#[#]" $(MAKEFILE_LIST) | sed -e 's/\\$$//' | awk 'BEGIN {FS = "[:=].*?#[#] "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""

.PHONY: update-version
update-version: ## update version from VERSION file in all Cargo.toml manifests
update-version: */Cargo.toml
	@VERSION=$$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1); \
	sed -i -E "s/^(rash\_.*version\s=\s)\"(.*)\"/\1\"$$VERSION\"/gm" */Cargo.toml && \
	cargo update -p rash_core -p rash_derive && \
	echo updated to version "$$VERSION" cargo files


.PHONY: cross
cross:	## install cross if needed
	@if [ "$(CARGO)" != "cargo" ]; then  \
		if [ "$${CARGO_TARGET_DIR}" != "$${CARGO_TARGET_DIR#/}" ]; then  \
			echo CARGO_TARGET_DIR should be relative for cross compiling; \
			exit 1; \
		fi; \
		cargo install cross; \
	fi

.PHONY: build
build:	cross
build:	## compile rash
	$(CARGO) build $(CARGO_BUILD_PARAMS)

.PHONY: lint
lint:	## lint code
	cargo clippy --locked --all-targets --all-features -- -D warnings
	cargo fmt -- --check

.PHONY: test
test: lint cross
test:	## run tests
	@$(CARGO) test $(CARGO_BUILD_PARAMS)

.PHONY: test-examples
test-examples:	## run examples and check exit code
	@for example in $$(find examples -not -path 'examples/envar-api-gateway/*' \
		-not -path 'examples/diff.rh' -not -path 'examples/dotfiles/*' -name '*.rh'); do \
		echo $$example; \
		$$example || exit 1; \
	done
	@echo
	@echo
	@echo
	@echo all good!

.PHONY: mdbook-rash
mdbook-rash:	## install mdbook_rash to create rash_book
	cargo install --locked --path mdbook_rash

.PHONY: book
book:	## create rash_book under rash_book/rash-sh.github.io
book:	mdbook-rash
	MDBOOK_BUILD__BUILD_DIR=$(BOOK_DIR)/rash-sh.github.io/docs/rash/$(VERSION) mdbook build rash_book

.PHONY: update-changelog
update-changelog:	## automatically update changelog based on commits
	git cliff -t v$(PROJECT_VERSION) -u -p CHANGELOG.md

.PHONY: release
release: cross
release:	## generate $(PKG_BASE_NAME).tar.gz with binary
	@$(CARGO) build --frozen --release $(CARGO_BUILD_PARAMS)
	@if command -v "upx" &> /dev/null; then \
		upx $(CARGO_TARGET_DIR)/$(CARGO_TARGET)/release/rash; \
	fi
	@tar -czf $(PKG_BASE_NAME).tar.gz -C $(CARGO_TARGET_DIR)/$(CARGO_TARGET)/release rash && \
	echo Released in $(CARGO_TARGET_DIR)/$(CARGO_TARGET)/release/rash;

.PHONY: publish
publish:	## publish crates
	@for package in $(shell find . -mindepth 2 -not -path './vendor/*' -name Cargo.toml -exec dirname {} \; | sort -r);do \
		cd $$package; \
		cargo publish; \
		cd -; \
	done;

.PHONY: images
images:	## build images
images: CARGO_TARGET=x86_64-unknown-linux-musl
images: release
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker buildx build -f $$DOCKERFILE \
			--build-arg "CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)" \
			--load \
			-t $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/Dockerfile//' | tr '.' '-'` \
			.; \
	done;

.PHONY: test-images
test-images:	## test images
test-images: images
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker run \
			-v $(shell pwd)/examples:/examples:ro \
			$(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/Dockerfile//' | tr '.' '-'` \
			/examples/builtins.rh; \
	done;

.PHONY: push-images
push-images:	## push images
push-images: images
	@for DOCKERFILE in $(DOCKERFILES);do \
		if [ "$$DOCKERFILE" = "Dockerfile" ]; then \
			DOCKER_EXTRA_ARGS="--platform linux/amd64,linux/arm64"; \
		else \
			DOCKER_EXTRA_ARGS=""; \
		fi; \
		docker buildx build -f $$DOCKERFILE \
			--build-arg "CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)" \
			$$DOCKER_EXTRA_ARGS \
			--push \
			-t $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/Dockerfile//' | tr '.' '-'` \
			.; \
	done;
