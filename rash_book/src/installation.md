---
title: Installation
weight: 3000
---

# Installation

The following steps install the latest stable version of `rash` into your system.

## Aur package

For ArchLinux users, exists an AUR package that it is maintained in this repository.
You can choose your favorite AUR package manager and just install the `rash` package.
E.g.:

```bash
yay -S rash
```

## Binary

If you are using Linux or macOs, open a terminal and enter the following command:

```bash
curl -s https://api.github.com/repos/rash-sh/rash/releases/latest \
    | grep browser_download_url \
    | grep $(uname -m) \
    | grep $(uname | tr '[:upper:]' '[:lower:]') \
    | grep -v musl \
    | cut -d '"' -f 4 \
    | xargs curl -s -L \
    | sudo tar xvz -C /usr/local/bin
```

The command downloads latest release binary and it to `/usr/loca/bin`.
Note that you might be prompted for your password.

## Cargo

If you prefer to use cargo for installation you always can do:

```bash
cargo install rash_core
```

## Docker

```bash
docker run --rm -v /usr/local/bin/:/output --entrypoint /bin/cp rustagainshell/rash:latest /bin/rash /output/
```
