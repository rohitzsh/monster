FROM --platform=linux/arm64 alpine:latest

WORKDIR /app

COPY target/aarch64-unknown-linux-musl/release/monster-agent /app/monster-agent
COPY ./build_arm64/go.sh /app/go.sh

CMD ["/app/go.sh"]
