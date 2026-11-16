####################################################################################################
## Base Stage - Configures fast linker and certificates for all stages
####################################################################################################
# La versión del toolchain está clavada y coincide con rust-toolchain.toml:
# `rust:bookworm` resolvería a la última stable y una release nueva podría romper
# el deploy sin que ningún commit haya cambiado.
FROM rust:1.98.1-bookworm AS base
RUN update-ca-certificates

# Install LLD linker and cmake (used by aws-lc-sys on non-prebuilt targets)
RUN apt-get update && apt-get install -y lld clang cmake && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C link-arg=-fuse-ld=lld"
ENV CARGO_HTTP_MULTIPLEXING=false
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

# rustfmt y clippy son compuertas de build, no herramientas opcionales.
RUN rustup component add rustfmt clippy

# cargo-chef is installed once here so planner and cacher reuse this cached layer
RUN cargo install cargo-chef --version 0.1.71

# cargo-deny escanea advisories de RUSTSEC, fuentes y dependencias comodín.
RUN cargo install cargo-deny --locked --version 0.20.2

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
RUN cargo chef cook --release --workspace --all-targets --recipe-path recipe.json && \
    cargo chef cook --release -p engine --all-features --all-targets --recipe-path recipe.json

####################################################################################################
## Builder Stage - builds only your source code
####################################################################################################
FROM base AS builder
WORKDIR /app

COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo

COPY . .

# Deploy gate: no image exists if any check is red, so a broken build cannot reach
# Cloud Run even through a manual deploy that bypasses an external CI pipeline.
# Orden deliberado: primero las compuertas baratas, luego las que compilan.
RUN cargo fmt --all --check
RUN bash scripts/audit.sh --quiet
RUN cargo clippy --workspace --release --all-targets -- -D warnings
RUN cargo clippy -p engine --all-features --release --all-targets -- -D warnings
# Tests unitarios y HTTP en memoria. Los de integración llevan `#[ignore]` porque necesitan un
# MongoDB desechable; aquí no hay ninguno y el build no depende de la red.
RUN cargo test --workspace --release
RUN cargo test -p engine --all-features --release

RUN cargo build --release -p service

# Supply-chain: advisories de RUSTSEC, crates yanked, versiones comodin y registries
# desconocidos. `licenses` queda fuera hasta validar la allowlist de deny.toml.
RUN cargo deny check advisories bans sources

####################################################################################################
## Final image Stage
####################################################################################################
FROM gcr.io/distroless/cc-debian12
WORKDIR /app

COPY --from=builder --chown=10001:10001 --chmod=755 /app/target/release/service ./service

USER 10001:10001

CMD ["/app/service"]
