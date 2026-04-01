FROM rust:1.85-alpine AS builder

RUN apk add --no-cache musl-dev pkgconfig openssl-dev gcc g++ make

WORKDIR /usr/src/lerke
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs
RUN cargo build --release || true
RUN rm -rf src

COPY . .
RUN touch src/main.rs src/lib.rs
RUN cargo build --release

FROM alpine:latest
COPY --from=builder /usr/src/lerke/target/release/lerke /usr/local/bin/lerke
COPY --from=builder /usr/src/lerke/static /app/static
COPY --from=builder /usr/src/lerke/migrations /app/migrations
WORKDIR /app
USER 1000
CMD ["lerke"]
