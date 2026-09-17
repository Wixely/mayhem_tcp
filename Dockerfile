FROM rust:1.85-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
COPY --from=build /build/target/release/mayhem_tcp /usr/local/bin/mayhem_tcp
EXPOSE 1234/tcp
ENTRYPOINT ["mayhem_tcp"]
CMD ["-a", "0.0.0.0"]
