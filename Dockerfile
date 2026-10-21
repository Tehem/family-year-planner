# syntax=docker/dockerfile:1

# --- 1. Front-end bundle -----------------------------------------------------------------------
FROM node:22-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# --- 2. Rust binary (embeds web/dist) ------------------------------------------------------------
# cargo-chef splits dependency compilation from the project's own code: the dependency layer is
# rebuilt only when Cargo.lock / Cargo.toml change, not on every source change.
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS chef
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p annee-server
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY --from=web /src/web/dist web/dist
RUN cargo build --release --locked -p annee-server \
    && strip target/release/annee

# --- 3. Runtime ----------------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/annee /usr/local/bin/annee
ENV BIND=0.0.0.0:8080 \
    APP_CONFIG=/config/config.toml \
    DATABASE_URL=sqlite:///data/annee.db \
    RUST_LOG=info
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/annee"]
CMD ["serve"]
