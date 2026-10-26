# syntax=docker/dockerfile:1

# The image contains the static musl binary from the GitHub Release. There is
# no package manager, shell, or libc inside: the attack engine is pure CPU and
# never needs network access.
FROM scratch

ARG TARGETARCH

LABEL org.opencontainers.image.title="password-cracking" \
      org.opencontainers.image.description="High-performance ZIP, PDF and Office password cracker" \
      org.opencontainers.image.url="https://github.com/LiangLliu/password-cracking" \
      org.opencontainers.image.source="https://github.com/LiangLliu/password-cracking" \
      org.opencontainers.image.licenses="MIT"

COPY dist/linux-${TARGETARCH}/password-cracking /usr/local/bin/password-cracking

ENTRYPOINT ["/usr/local/bin/password-cracking"]
CMD ["--help"]
