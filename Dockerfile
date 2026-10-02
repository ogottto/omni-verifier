FROM debian:bookworm-slim

# Install SSL certificates and curl for health check
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Run as non-root user
RUN groupadd -g 10001 appgroup && \
    useradd -u 10001 -g appgroup -s /bin/sh appuser

USER 10001:10001

WORKDIR /app

COPY --chown=appuser:appgroup target/release/omni-verifier /usr/local/bin/omni-verifier

ENV HOST=0.0.0.0
ENV PORT=8099
ENV RUST_LOG=info,omni_verifier=debug,actix_web=info

EXPOSE 8099

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8099/health || exit 1

ENTRYPOINT ["/usr/local/bin/omni-verifier"]
