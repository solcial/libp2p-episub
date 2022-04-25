# syntax=docker/dockerfile:1.3
FROM nwtgck/rust-musl-builder:latest AS rust-build
COPY --chown=rust:rust . /code
RUN --mount=type=cache,target=/home/rust/.cargo/registry,uid=1000,gid=1000 --mount=type=cache,target=/code/target,uid=1000,gid=1000 cd /code && cargo build --release --target x86_64-unknown-linux-musl
RUN --mount=type=cache,target=/home/rust/.cargo/registry,uid=1000,gid=1000 --mount=type=cache,target=/code/test/audit/target,uid=1000,gid=1000 cd /code/test/audit && cargo build --release --target x86_64-unknown-linux-musl && cp /code/test/audit/target/x86_64-unknown-linux-musl/release/audit-node /home/rust
RUN --mount=type=cache,target=/home/rust/.cargo/registry,uid=1000,gid=1000 --mount=type=cache,target=/code/test/node/target,uid=1000,gid=1000 cd /code/test/node && cargo build --release --target x86_64-unknown-linux-musl && cp /code/test/node/target/x86_64-unknown-linux-musl/release/episub-node /home/rust


FROM alpine:latest
WORKDIR /home/rust
COPY --from=rust-build /home/rust/audit-node .
COPY --from=rust-build /home/rust/episub-node .
COPY --from=rust-build /code/test/audit/assets ./assets

EXPOSE 4001
EXPOSE 80
