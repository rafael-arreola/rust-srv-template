####################################################################################################
## Base Stage - Configures fast linker and certificates for all stages
####################################################################################################
FROM rust:bookworm AS base
RUN update-ca-certificates

# Install LLD linker to make compiling and linking significantly faster across all stages
RUN apt-get update && apt-get install -y lld clang && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C link-arg=-fuse-ld=lld"

# cargo-chef is installed once here so planner and cacher reuse this cached layer
RUN cargo install cargo-chef

####################################################################################################
## Planner Stage - generates the dependency recipe
####################################################################################################
FROM base AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

####################################################################################################
## Cacher Stage - builds only dependencies (cached between builds)
####################################################################################################
FROM base AS cacher
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

####################################################################################################
## Builder Stage - builds only your source code
####################################################################################################
FROM base AS builder
WORKDIR /app

COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo

COPY . .

RUN cargo build --release

# Deploy gate: no image exists if the suite is red, so a broken build cannot
# reach Cloud Run even through a manual deploy that bypasses an external CI pipeline.
# `--release` reuses the artifacts just built instead of recompiling in debug.
RUN cargo test --release --all-targets

# Extract the compiled binary dynamically. Symbols are already stripped by
# `profile.release.strip = true` in Cargo.toml, so no extra `strip` pass is needed.
RUN BIN_PATH=$(find target/release -maxdepth 1 -type f -executable ! -name "*.so" ! -name "*.d" | head -n 1) && \
    cp "$BIN_PATH" /app/target/release/service

####################################################################################################
## Final image Stage
####################################################################################################
FROM gcr.io/distroless/cc-debian12
WORKDIR /app

COPY --from=builder --chown=10001:10001 --chmod=755 /app/target/release/service ./service

USER 10001:10001

CMD ["/app/service"]
