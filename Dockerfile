ARG RUST_VERSION=1.99.1
FROM rust:${RUST_VERSION}-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p forgeplan
FROM debian:bookworm-slim
RUN mkdir /data && chown 65532:65532 /data
COPY --from=build /src/target/release/forgeplan /usr/local/bin/forgeplan
USER 65532:65532
WORKDIR /data
VOLUME ["/data"]
EXPOSE 8787
ENV FORGEPLAN_BIND=0.0.0.0
ENTRYPOINT ["forgeplan"]
CMD ["workspace", "8787", "/data/forgeplan.sqlite"]
