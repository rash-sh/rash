FROM rustagainshell/rash AS rash
FROM nginx
COPY --from=rash /bin/rash /bin

COPY entrypoint.rh /
ENTRYPOINT ["/entrypoint.rh"]
