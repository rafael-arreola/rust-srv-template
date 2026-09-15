####################################################################################################
## Base Stage - Configures fast linker and certificates for all stages
####################################################################################################
# La version del toolchain esta clavada y debe coincidir con rust-toolchain.toml:
# `rust:bookworm` resolveria a la ultima stable y una release nueva podria romper
# el deploy sin que ningun commit haya cambiado.
FROM rust:1.98.1-bookworm AS base
RUN update-ca-certificates

# Install LLD linker to make compiling and linking significantly faster across all stages
RUN apt-get update && apt-get install -y lld clang && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C link-arg=-fuse-ld=lld"

# rustfmt y clippy son compuertas de build, no herramientas opcionales.
RUN rustup component add rustfmt clippy

# cargo-chef is installed once here so planner and cacher reuse this cached layer
RUN cargo install cargo-chef --version 0.1.71

# cargo-deny escanea advisories de RUSTSEC, fuentes y dependencias comodin.
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
RUN cargo chef cook --release --recipe-path recipe.json

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
RUN cargo fmt --check
RUN bash scripts/audit.sh --quiet
RUN cargo clippy --release --all-targets -- -D warnings

RUN cargo build --release

# Supply-chain: advisories de RUSTSEC, crates yanked, versiones comodin y registries
# desconocidos. `licenses` queda fuera hasta validar la allowlist de deny.toml.
RUN cargo deny check advisories bans sources

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
