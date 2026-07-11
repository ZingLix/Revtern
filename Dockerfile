FROM node:26-bookworm-slim AS web-builder
WORKDIR /app

ARG VITE_API_BASE_URL=
ENV VITE_API_BASE_URL=${VITE_API_BASE_URL}

COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/package.json
COPY packages/api-client/package.json packages/api-client/package.json
COPY packages/types/package.json packages/types/package.json
RUN npm ci

COPY tsconfig.base.json ./
COPY apps/web ./apps/web
COPY packages ./packages
RUN npm run build -w @revtern/web

FROM rust:1.96-bookworm AS api-builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p revtern-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 wget \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=api-builder /app/target/release/revtern-api /usr/local/bin/revtern-api
COPY --from=web-builder /app/apps/web/dist /app/web

ENV REVTERN_BIND=0.0.0.0:3000
ENV REVTERN_WEB_DIST=/app/web

EXPOSE 3000
CMD ["revtern-api"]
