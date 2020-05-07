# rash

Declarative shell using Rust native bindings scripting inspired in [Ansible](https://www.ansible.com/)

## Advantages over alpine/bash:

- More security (choose your flavour)
- Declarative over bash imperative language.
- Not need more than kernel

## Examples

### add_ssh_key.rh

```yaml
#!/bin/rash
---
- name: .ssh create if not exists
  file:
    path: "~/.ssh"
    mode: 0600
    state: directory

- name: copy ssh key
  host: "{{ $1 }}"
  lineinfile:
    path: "~/.ssh/authorized_keys"
    line: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAeGW1P62P2rsq0XqbRaDKBcXZUPRklo0L1EyR30CwoP agil@z800
```

### entrypoint.rh

```yaml
#!/bin/rash
---
- name: get secrets
  request:
    url: https://vault/my/secret
  register: response_my_secret

- name: save secret
  copy:
    content: "{{ response_my_secret.json }}"
    dest: /my/secret/path

- name: run app as PID 1
  exec: /usr/bin/myapp -c
```
