DOCKERFILES ?= $(shell find . -maxdepth 1 -name 'Dockerfile*' -not -name '*.dockerignore')
IMAGE_NAME ?= rustagainshell/rash
IMAGE_VERSION ?= latest

BOOK_DIR ?= ./
CARGO_TARGET ?= x86_64-unknown-linux-gnu
PKG_BASE_NAME ?= rash-${CARGO_TARGET}
VERSION ?= master

CARGO_TARGET_DIR ?= target

.DEFAULT: help
.PHONY: help
help:	## Show this help menu.
	@echo "Usage: make [TARGET ...]"
	@echo ""
	@@egrep -h "#[#]" $(MAKEFILE_LIST) | sed -e 's/\\$$//' | awk 'BEGIN {FS = "[:=].*?#[#] "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""

.PHONY: update-version
update-version: ## update version from VERSION file in all Cargo.toml manifests
update-version: */Cargo.toml
	@VERSION=$$(cat VERSION); \
	sed -i "0,/^version\ \= .*$$/{s//version = \"$$VERSION\"/}" */Cargo.toml && \
	sed -i -E "s/^(rash\_.*version\s=\s)\"(.*)\"/\1\"$$VERSION\"/gm" */Cargo.toml && \
	cargo update -p rash_core -p rash_derive && \
	echo updated to version "$$(cat VERSION)" cargo files

.PHONY: build
build:	## compile rash
build:
	cargo build --release

.PHONY: lint
lint:	## lint code
lint:
	cargo clippy -- -D warnings
	cargo fmt -- --check

.PHONY: test
test:	## run tests
test: lint
	cargo test

.PHONY: test-examples
test-examples:	## run examples and check exit code
	@for example in $$(find examples -not -path 'examples/envar-api-gateway/*' -not -path 'examples/diff.rh'  -name '*.rh'); do \
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

.PHONY: tag
tag:	## create a tag using version from VERSION file
	PROJECT_VERSION=$$(cat VERSION); \
	git tag -s v$${PROJECT_VERSION}  -m "v$${PROJECT_VERSION}" && \
	git push origin v$${PROJECT_VERSION}

.PHONY: release
release: CARGO_USE_CROSS ?= $(IMAGES_CARGO_USE_CROSS)
release:	## generate $(PKG_BASE_NAME).tar.gz with binary
	@if [ "$(CARGO_USE_CROSS)" = "true" ]; then  \
		if [ "$${CARGO_TARGET_DIR}" != "$${CARGO_TARGET_DIR#/}" ]; then  \
			echo CARGO_TARGET_DIR should be relative for cross compiling; \
			exit 1; \
		fi; \
		cargo install cross; \
		cross build --target-dir $(shell pwd)/$(CARGO_TARGET_DIR) \
			--target=$(CARGO_TARGET) --release; \
	else \
		cargo build --frozen --release --target ${CARGO_TARGET}; \
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
images: IMAGES_CARGO_USE_CROSS=true
images: release
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker build -f $$DOCKERFILE \
			--build-arg "CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)" \
			-t $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'` \
			.; \
	done;

.PHONY: test-images
test-images:	## test images
test-images: images
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker run \
			-v $(shell pwd)/examples:/examples:ro \
			$(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'` \
			/examples/builtins.rh; \
	done;

.PHONY: push-images
push-images:	## push images
push-images: images
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker push $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'`;\
	done;
