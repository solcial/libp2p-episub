FROM nwtgck/rust-musl-builder:latest AS rust-build
ADD . /code
RUN sudo chown -R rust /code
RUN cd /code && cargo build --release --target x86_64-unknown-linux-musl
RUN cd /code/test/audit && cargo build --release --target x86_64-unknown-linux-musl
RUN cd /code/test/node && cargo build --release --target x86_64-unknown-linux-musl


FROM alpine:latest
WORKDIR /home
COPY --from=rust-build /code/test/audit/target/x86_64-unknown-linux-musl/release/audit-node .
COPY --from=rust-build /code/test/node/target/x86_64-unknown-linux-musl/release/episub-node .
COPY --from=rust-build /code/test/audit/assets ./assets

EXPOSE 4001
EXPOSE 80
