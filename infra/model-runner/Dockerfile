FROM node:24-bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && npm install --global --omit=dev --no-audit --no-fund opencode-ai@1.18.10 \
    && opencode --version \
    && npm cache clean --force \
    && groupadd --gid 10001 modelrunner \
    && useradd --uid 10001 --gid 10001 --no-create-home --home-dir /tmp/runner/home modelrunner \
    && mkdir -p /etc/opencode /opt/model-runner /work \
    && chown 10001:10001 /work
COPY infra/model-runner/opencode.json /etc/opencode/opencode.json
COPY infra/model-runner/start.mjs infra/model-runner/healthcheck.mjs /opt/model-runner/
USER 10001:10001
WORKDIR /work
EXPOSE 4096
ENTRYPOINT ["node", "/opt/model-runner/start.mjs"]
HEALTHCHECK --interval=5s --timeout=4s --start-period=20s --retries=20 \
  CMD ["node", "/opt/model-runner/healthcheck.mjs"]
