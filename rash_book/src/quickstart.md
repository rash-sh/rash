# Quickstart

Add to Dockerfile `rash` binary and enjoy it!.

```dockerfile

FROM pando85/rash AS rash

FROM base_image

COPY --from=rash /bin/rash /bin

RUN do things

.
.
.

COPY entrypoint.rh /
ENTRYPOINT ["/entrypoint.rh"]

```

Create your first `entrypoint.rh`:

```yaml
#!/bin/rash

- command: myapp
  transfer_pid_1: true

```
