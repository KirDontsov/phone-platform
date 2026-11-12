FROM node:24-bookworm-slim@sha256:01d566e8d94d8c619591d78652babfccd3c6992a3dffa9ed5ab39e11cf6c70e9
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 assistant && useradd --uid 10001 --gid 10001 --no-create-home assistant \
    && mkdir -p /opt/assistant /var/lib/assistant-auth && chown 10001:10001 /var/lib/assistant-auth
WORKDIR /opt/assistant
COPY infra/assistant-runner/package.json infra/assistant-runner/package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund \
    && node node_modules/@openai/codex/bin/codex.js --version \
    && npm cache clean --force
COPY infra/assistant-runner/vendor ./vendor
COPY infra/assistant-runner/src ./src
COPY infra/assistant-runner/tests ./tests
USER 10001:10001
EXPOSE 8091
ENTRYPOINT ["node", "src/server.mjs"]
HEALTHCHECK --interval=5s --timeout=4s --start-period=15s --retries=20 CMD ["node", "src/healthcheck.mjs"]
