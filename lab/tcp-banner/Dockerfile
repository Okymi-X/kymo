FROM docker.io/library/busybox:stable
USER 1000
EXPOSE 9000
# A plain TCP service, not HTTP: every connection is greeted with the flag.
CMD ["sh", "-c", "while true; do echo 'KYMO{raw_tcp}' | nc -l -p 9000; done"]
