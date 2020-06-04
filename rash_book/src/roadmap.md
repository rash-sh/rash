# Roadmap <!-- omit in toc -->

The projects Roadmap is defined in our
[Concept Map](https://mind42.com/mindmap/f299679e-8dc5-48d8-b0f0-4d65235cdf56). Some more
concrete examples can be found below.

These are just some ideas about the possibilities of `rash`.

- [Lookups](#lookups)
  - [S3](#s3)
  - [vault](#vault)
  - [etcd](#etcd)
- [Modules](#modules)
  - [Copy](#copy)
  - [S3](#s3-1)
  - [Template](#template)
- [Filters](#filters)
  - [jmespath](#jmespath)

## Lookups

### S3

`s3.rh`:
```yaml
#!/bin/rash

- name: file from s3
  template:
    content: "{{ lookup('s3', bucket='mybucket', object='config.json.j2')}}"
    dest: /myapp/config.json
    mode: 0400

- name: launch docker CMD
  command: {{ input.args }}
  transfer_pid_1: yes
```

### vault

`vault.rh`:
```yaml
#!/bin/rash

- name: launch docker CMD
  command: {{ input.args }}
  transfer_pid_1: yes
  env:
    APP1_API_KEY: "{{ lookup('vault', env.VAULT_SECRET_PATH ) }}"
```

### etcd

`etcd.rh`:
```yaml
#!/bin/rash

- name: launch docker CMD
  command: {{ input.args }}
  transfer_pid_1: yes
  env:
    APP1_API_KEY: "{{ lookup('etcd', env.VAULT_SECRET_PATH ) }}"
```

## Modules

### Copy

```yaml
#!/bin/rash

- copy:
    content: "{{ lookup('etcd', env.MYAPP_CONFIG_ETCD_PATH)}}"
    dest: /myapp/config.json
    mode: 0400
```

### S3

`s3.rh`:
```yaml
#!/bin/rash

- name: file from s3
  s3:
    bucket: mybucket
    object: "{{ env.MYBUNDLE_S3_PATH }}"
    dest: /myapp/i18n/bundle.json
    mode: 0400

- name: launch docker CMD
  command: {{ input.args }}
  transfer_pid_1: yes
```

### Template

```yaml
#!/bin/rash

- template:
    content: "{{ lookup('s3', env.MYAPP_CONFIG_J2_TEMPLATE_S3_PATH)}}"
    dest: /myapp/config.json
    mode: 0400
```

## Filters

### [jmespath](https://docs.rs/jmespath/0.2.0/jmespath/)

```yaml
#!/bin/bash

- name: get some data
  uri:
    url: https://api.example.com/v1/my_data
  register: my_data

- set_vars:
    my_ips: "{{ my_data.json | json_query('[*].ipv4.address') }}"

```
