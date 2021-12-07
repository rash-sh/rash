DOCKERFILES ?= $(shell find . -maxdepth 1 -name 'Dockerfile*')
IMAGE_NAME ?= rustagainshell/rash
IMAGE_VERSION ?= latest

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

.PHONY: build-images
build-images:	## build images
build-images:
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker build -f $$DOCKERFILE \
			-t $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'` \
			.; \
	done;

.PHONY: test-images
test-images:	## test images
test-images: build-images
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker run \
			-v $(shell pwd)/test:/test:ro \
			$(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'` \
			/test/run.rh; \
	done;

.PHONY: push-images
push-images:	## push images
push-images: build-images
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker push $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'`;\
	done;

.PHONY: update-version
update-version: ## update version from VERSION file in all Cargo.toml manifests
update-version: */Cargo.toml
	@VERSION=`cat VERSION`; sed -i "0,/^version\ \= .*$$/{s//version = \"$$VERSION\"/}" */Cargo.toml
	@echo updated to version "`cat VERSION`" cargo files

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

.PHONY: mdbook-rash
mdbook-rash:	## install mdbook_rash to create rash_book
	cd mdbook_rash && \
	cargo install --path .

.PHONY: book
book:	## create rash_book under rash_book/rash-sh.github.io
book:	mdbook-rash
	MDBOOK_BUILD__BUILD_DIR=rash-sh.github.io/docs/rash/$(VERSION) mdbook build rash_book

.PHONY: tag
tag:	## create a tag using version from VERSION file
	PROJECT_VERSION=$$(cat VERSION); \
	git tag -s v$${PROJECT_VERSION} && \
	git push origin v$${PROJECT_VERSION}

.PHONY: release
release:	## generate vendor.tar.gz and $(PKG_BASE_NAME).tar.gz
	cargo vendor
	tar -czf vendor.tar.gz vendor
	cargo build --frozen --release --all-features --target ${CARGO_TARGET}
	tar -czf $(PKG_BASE_NAME).tar.gz -C $(CARGO_TARGET_DIR)/$(CARGO_TARGET)/release rash
	@echo Released in $(CARGO_TARGET_DIR)/$(CARGO_TARGET)/release/rash

.PHONY: publish
publish:	## publish crates
	@for package in $(shell find . -mindepth 2 -not -path './vendor/*' -name Cargo.toml -exec dirname {} \; | sort -r);do \
		cd $$package; \
		cargo publish; \
		cd -; \
	done;
