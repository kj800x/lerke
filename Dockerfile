# Build Stage
FROM rust:1.93-alpine AS builder
WORKDIR /usr/src/
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static gcc g++ make

WORKDIR /usr/src
RUN USER=root cargo new lerke
WORKDIR /usr/src/lerke
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

COPY src ./src
COPY static ./static
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

# Runtime Stage
FROM alpine:latest AS runtime
WORKDIR /app
COPY --from=builder /usr/src/lerke/target/release/lerke /usr/local/bin/lerke
COPY --from=builder /usr/src/lerke/static /app/static
COPY --from=builder /usr/src/lerke/migrations /app/migrations
USER 1000
CMD ["lerke"]
