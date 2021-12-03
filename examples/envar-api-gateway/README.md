# envar-api-gateway

Silly example to emulate a simple endpoint that could be used as silly API gateway.

## how to

- build: `docker build -t envar-api-gateway .`
- run: `docker run -e DOMAINS=rash.sh,buildpacks.io -p 80:80 --rm envar-api-gateway`
- test: `curl 127.0.0.1/rash; curl 127.0.0.1/buildpacks`
