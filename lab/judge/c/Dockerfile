# A language for coding challenges: C. The compile step runs once in a writable /work and the
# tests then run the program it built, read only.
#
#   docker build -t localhost/kymo-judge/c:1 lab/judge/c
FROM alpine:3.20
RUN apk add --no-cache gcc musl-dev
