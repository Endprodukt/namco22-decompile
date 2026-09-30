# rrn1-server

Lobby + relay server for Rave Racer online play (protocol: `../NETPLAY.md`). Rust, standard library only.

```
cargo build --release
./target/release/rrn1-server                 # UDP 27750 on every interface
./target/release/rrn1-server --port 27750 --bind 0.0.0.0 --name "My server" --max-per-ip 4 --quiet
cargo test                                   # protocol conformance tests (no sockets, fake clock)
```

Players enter `host` or `host:port` on the game's Online page (or `RR_NET_SERVER=host:port`). It also answers
LAN discovery, so a server on the local network shows up under *Find LAN games*. Open UDP 27750 in the firewall.

## Run it in Docker (the easy way)

On any Linux machine with Docker and a public address (a small VPS is plenty):

```
git clone https://github.com/spacestate1/namco22-decompile
cd namco22-decompile/raverace/server
docker compose up -d --build
```

That builds the server and keeps it running (and restarts it after a reboot). Then open **UDP 27750** in the machine's
firewall. Players type your IP or domain name in the game's Online > Internet game field and press Connect.

Options are environment variables: `RRN1_NAME="My server" RRN1_PORT=27750 RRN1_MAX_PER_IP=4 docker compose up -d --build`
(the name is at most 16 bytes and shows in the LAN game list). Without compose:

```
docker build -t rrn1-server .
docker run -d --init --name rrn1 --restart unless-stopped --network host -e RRN1_NAME="My server" rrn1-server
```

Host networking is deliberate: a UDP relay must see every player's real address (the per-IP limit uses it), and LAN
discovery needs broadcasts. Logs: `docker logs rrn1`. Stop: `docker compose down`.

The game can also host a lobby itself (Online > Host a LAN game); this server is for a machine that is always on.
Hardening beyond the contract: FRAMEs must be exactly 59 bytes, addresses without a slot are limited to 30 datagrams/s,
a rejected HELLO gets a 17-byte reply, one IP holds at most `--max-per-ip` slots. There is no authentication yet.
