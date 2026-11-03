FROM docker.io/library/busybox:stable
COPY www /www
USER 1000
EXPOSE 8000
CMD ["httpd", "-f", "-p", "8000", "-h", "/www"]
