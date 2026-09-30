# rrn1-server: the Rave Racer online lobby/relay (UDP 27750) in a small image.
#   docker build -t rrn1-server .
#   docker run -d --init --name rrn1 --restart unless-stopped --network host rrn1-server
# Options are environment variables (see docker-compose.yml): RRN1_PORT, RRN1_NAME, RRN1_MAX_PER_IP.
FROM rust:1-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN useradd --system --no-create-home rrn1
COPY --from=build /src/target/release/rrn1-server /usr/local/bin/rrn1-server
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
USER rrn1
ENV RRN1_PORT=27750 RRN1_NAME="Rave Racer" RRN1_MAX_PER_IP=4
EXPOSE 27750/udp
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
