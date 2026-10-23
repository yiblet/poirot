FROM rust:1.90.0-bookworm AS build
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY LICENSES ./LICENSES
COPY src ./src
COPY crates ./crates
COPY benchmarks/rust ./benchmarks/rust
# Bundle PCRE2 so the slim runtime does not need a native regex shared library.
RUN PCRE2_SYS_STATIC=1 cargo build --locked --release --manifest-path benchmarks/rust/Cargo.toml -p poirot-speed

# Retain upstream notices and license texts alongside the benchmark executable.
RUN mkdir -p /licenses && rustc --version > /licenses/rustc-version.txt && \
    cp benchmarks/rust/Cargo.lock /licenses/ && \
    cp -r LICENSES /licenses/poirot && \
    find /usr/local/cargo/registry/src -type f \( -iname '*license*' -o -iname '*copying*' -o -iname '*notice*' \) \
    -exec cp --parents {} /licenses/ \;

FROM debian:bookworm-slim
COPY --from=build /source/benchmarks/rust/target/release/poirot-speed /usr/local/bin/benchmark
COPY --from=build /licenses /licenses
USER 65534:65534
ENTRYPOINT ["benchmark"]
CMD ["overlay"]
