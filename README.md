<img src="https://raw.githubusercontent.com/rash-sh/rash/master/artwork/shelly.svg" width="20%" height="auto" />

# rash

![Build status](https://img.shields.io/github/workflow/status/rash-sh/rash/Rust/master)
[![Rash Docker image](https://img.shields.io/docker/image-size/rustagainshell/rash?sort=semver)](https://cloud.docker.com/repository/docker/rustagainshell/rash)
[![Documentation](https://docs.rs/rash_core/badge.svg)](https://docs.rs/rash_core)
[![crates.io](https://img.shields.io/crates/v/rash_core)](https://crates.io/crates/rash_core)
[![concept-map](https://img.shields.io/badge/design-concept--map-blue)](https://mind42.com/mindmap/f299679e-8dc5-48d8-b0f0-4d65235cdf56)
![Rash license](https://img.shields.io/github/license/rash-sh/rash)
[![Rash Aur package](https://img.shields.io/aur/version/rash)](https://aur.archlinux.org/packages/rash)

Declarative shell scripting using Rust native bindings inspired by [Ansible](https://www.ansible.com/)

## Getting Started & Documentation

For installation and usage, see our
[Documentation](https://rash.sh/docs/rash/master/getting-started.html#quickstart).

## Why Rash

Manage your docker entrypoints in a declarative style.

If you:

- think that long bash scripts are difficult to maintain
- love Ansible syntax to setup your environments

Or use it for your local scripts!

Then keep on reading.

Here is Rash!

### Declarative vs imperative

Imperative: `entrypoint.sh`:

```bash
#!/bin/bash
set -e

REQUIRED_PARAMS="
VAULT_URL
VAULT_ROLE_ID
VAULT_SECRET_ID
VAULT_SECRET_PATH
"

for required in $REQUIRED_PARAMS ; do
  [[ -z "${!required}" ]] && echo "$required IS NOT DEFINED" && exit 1
done

echo "[$0] Logging into Vault..."
VAULT_TOKEN=$(curl -s $VAULT_URL/v1/auth/approle/login \
--data '{"role_id": "'$VAULT_ROLE_ID'","secret_id": "'$VAULT_SECRET_ID'"}' \
| jq -r .auth.client_token)

echo "[$0] Getting Samuel API key from Vault..."
export APP1_API_KEY=$(curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
$VAULT_URL/v1/$VAULT_SECRET_PATH | jq -r .data.api_key)


exec "$@"
```

Declarative: `entrypoint.rh`

```yaml
#!/bin/rash

- name: Verify input parameters
  assert:
    that:
      - env.VAULT_URL is defined
      - env.VAULT_ROLE_ID is defined
      - env.VAULT_SECRET_ID is defined
      - env.VAULT_SECRET_PATH is defined

- name: launch docker CMD
  command: {{ rash.argv }}
  transfer_pid: yes
  env:
    APP1_API_KEY: "{{ lookup('vault', env.VAULT_SECRET_PATH ) }}"
```

### Lightness

All you need to run Rash is a Linux kernel!

You can use it in your favorite IoT chips running Linux or in containers from scratch!

## Status

Stable API with few modules. It will be growing, stay tuned.

## Who is using `rash`

- A production ready [php-fpm](https://github.com/dcarrillo/docker-phpfpm) docker image
- [hs110 prometheus exporter](https://github.com/sdelrio/hs110-prometheus-exporter)
