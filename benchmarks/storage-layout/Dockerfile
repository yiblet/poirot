FROM rust:1.90.0-bookworm AS build
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY LICENSES ./LICENSES
COPY src ./src
COPY crates ./crates
COPY benchmarks/storage-layout ./benchmarks/storage-layout
RUN cargo build --locked --release --manifest-path benchmarks/storage-layout/Cargo.toml

# Preserve dependency notices alongside the diagnostic executable.
RUN mkdir -p /licenses && rustc --version > /licenses/rustc-version.txt && \
    cp benchmarks/storage-layout/Cargo.lock /licenses/ && \
    cp -r LICENSES /licenses/poirot && \
    find /usr/local/cargo/registry/src -type f \( -iname '*license*' -o -iname '*copying*' -o -iname '*notice*' \) \
    -exec cp --parents {} /licenses/ \;

FROM debian:bookworm-slim
COPY --from=build /source/benchmarks/storage-layout/target/release/poirot-storage-layout /usr/local/bin/benchmark
COPY --from=build /licenses /licenses
USER 65534:65534
ENTRYPOINT ["benchmark"]
