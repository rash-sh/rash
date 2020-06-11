.PHONY: help build-images push-images update-version build mdbook-rash book

IMAGE_NAME ?= rustagainshell/rash
IMAGE_VERSION ?= latest

VERSION ?= master
DOCKERFILES ?= $(shell find . -name 'Dockerfile*')

CARGO_TARGET_DIR ?= target

.DEFAULT: help
help:	## Show this help menu.
	@echo "Usage: make [TARGET ...]"
	@echo ""
	@@egrep -h "#[#]" $(MAKEFILE_LIST) | sed -e 's/\\$$//' | awk 'BEGIN {FS = "[:=].*?#[#] "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""

build-images:	## build images
build-images:
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker build -f $$DOCKERFILE \
			-t $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'` \
			. &\
	done; \
	wait

test-images:	## test images
test-images: build-images
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker run \
			-v $(shell pwd)/test:/test:ro \
			$(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'` \
			/test/run.rh; \
	done;

push-images:	## push images
push-images: build-images
	@for DOCKERFILE in $(DOCKERFILES);do \
		docker push $(IMAGE_NAME):$(IMAGE_VERSION)`echo $${DOCKERFILE} | sed 's/\.\/Dockerfile//' | tr '.' '-'` &\
	done; \
	wait

update-version: ## update version from VERSION file in all Cargo.toml manifests
update-version: */Cargo.toml
	@VERSION=`cat VERSION`; sed -i "0,/^version\ \= .*$$/{s//version = \"$$VERSION\"/}" */Cargo.toml
	@echo updated to version "`cat VERSION`" cargo files

build:	## compile rash
build:
	cargo build --release

lint:	## lint code
lint:
	cargo clippy -- -D warnings
	cargo fmt -- --check

test:	## run tests
test: lint
	cargo test

mdbook-rash:	## install mdbook_rash to create rash_book
	cd mdbook_rash && \
	cargo install --path .

book:	## create rash_book under rash_book/rash-sh.github.io
book:	mdbook-rash
	cd rash_book && \
	MDBOOK_BUILD__BUILD_DIR=rash-sh.github.io/docs/rash/$(VERSION) mdbook build

release:	## generate vendor.tar.gz and rash-v${VERSION}-x86_64-unkown-linux-gnu.tar.gz
	cargo vendor
	tar -czf vendor.tar.gz vendor
	cargo build --release
	tar -czf rash-x86_64-unkown-linux-gnu.tar.gz -C $(CARGO_TARGET_DIR)/release rash

publish:	## publish crates
	@for package in $(shell find . -mindepth 2 -not -path './vendor/*' -name Cargo.toml -exec dirname {} \; | sort -r);do \
		cd $$package; \
		cargo publish; \
		cd -; \
	done;
