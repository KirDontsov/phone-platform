FROM rust:1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS build
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends libpq-dev pkg-config && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY phone-api ./phone-api
COPY phone-worker ./phone-worker
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/app/target cargo build --release --locked -p phone-worker && cp /app/target/release/phone-worker /tmp/phone-worker
FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc
RUN apt-get update && apt-get install -y --no-install-recommends libpq5 ca-certificates ffmpeg && rm -rf /var/lib/apt/lists/* && useradd -u 10001 -m worker
WORKDIR /app
COPY --from=build /tmp/phone-worker /usr/local/bin/phone-worker
RUN mkdir -p /media && chown worker:worker /media
USER 10001
HEALTHCHECK --interval=15s --timeout=5s --start-period=30s CMD ["phone-worker","healthcheck"]
CMD ["phone-worker"]
