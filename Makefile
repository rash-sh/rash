.PHONY: help build-images push-images update-version build

IMAGE_NAME ?= rustagainshell/rash
IMAGE_VERSION ?= latest

DOCKERFILES ?= $(shell find . -name 'Dockerfile*')

.DEFAULT: help
help:
	@fgrep -h "##" $(MAKEFILE_LIST) | fgrep -v fgrep | sed -e 's/\\$$//' | sed -e 's/##/\n\t/'

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
push-images:
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
