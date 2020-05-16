<img src="https://raw.githubusercontent.com/pando85/rash/master/artwork/logo.png" width="20%" height="auto" />

# rash
![](https://img.shields.io/github/workflow/status/pando85/rash/Rust/master) [![](https://img.shields.io/badge/design-concept--map-blue)](https://mind42.com/mindmap/f299679e-8dc5-48d8-b0f0-4d65235cdf56) ![](https://img.shields.io/github/license/pando85/rash)

Declarative shell scripting using Rust native bindings inspired in [Ansible](https://www.ansible.com/)

## Why Rash?

Manage your docker entrypoints in a declarative style.

If you think that:

- long bash scripts are difficult to maintain
- you love Ansible syntax to setup your environments

Here is Rash!

### Declarative vs imperative

`entrypoint.sh`:
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

`entrypoint.rh`
```yaml
#!/bin/rash

- name: Verify input parameters
  assert:
    that:
      - VAULT_URL is defined
      - VAULT_ROLE_ID is defined
      - VAULT_SECRET_ID is defined
      - VAULT_SECRET_PATH is defined

- name: launch docker CMD
  command: {{ input.args }}
  transfer_ownership: yes
  env:
    APP1_API_KEY: "{{ lookup('vault', env.VAULT_SECRET_PATH ) }}"
```

### Lightness

Not need more than a linux kernel to run!

You could use it in your favorite IoT chips running Linux or in containers from scratch!

## Status

![Examples](https://media.giphy.com/media/YqQQGUib5yzNM2GvFe/giphy.gif)

[Jinja2](https://tera.netlify.app/docs/#templates) template engine support by [Tera](https://github.com/Keats/tera).

Current [modules](./rash_core/src/modules/)

## Roadmap

Roadmap is defined in our Concept Map but some more concrete examples could be found below:

### Lookups

#### S3

`s3.rh`:
```yaml
#!/bin/rash

- name: file from s3
  copy:
    content: "{{ lookup('s3', env.MYBUNDLE_S3_PATH) }}"
    dest: /myapp/i18n/bundle.json
    mode: 0400

- name: launch docker CMD
  command: {{ input.args }}
  transfer_ownership: yes

```

#### vault

`vault.rh`:
```yaml
#!/bin/rash

- name: launch docker CMD
  command: {{ input.args }}
  transfer_ownership: yes
  env:
    APP1_API_KEY: "{{ lookup('vault', env.VAULT_SECRET_PATH ) }}"
```

#### etcd

`etcd.rh`:
```yaml
#!/bin/rash

- name: launch docker CMD
  command: {{ input.args }}
  transfer_ownership: yes
  env:
    APP1_API_KEY: "{{ lookup('etcd', env.VAULT_SECRET_PATH ) }}"
```

### Modules

#### Copy

```yaml
#!/bin/rash

- copy:
    content: "{{ lookup('etcd', env.MYAPP_CONFIG_ETCD_PATH)}}"
    dest: /myapp/config.json
    mode: 0400
```

#### Template

```yaml
#!/bin/rash

- template:
    content: "{{ lookup('s3', env.MYAPP_CONFIG_J2_TEMPLATE_S3_PATH)}}"
    dest: /myapp/config.json
    mode: 0400
```

### Filters

#### [jmespath](https://docs.rs/jmespath/0.2.0/jmespath/)

```yaml
#!/bin/bash

- name: get some data
  uri:
    url: https://api.example.com/v1/my_data
  register: my_data

- set_facts:
    my_ips: "{{ my_data.json | json_query('[*].ipv4.address') }}"

```
