# RRN1 — Rave Racer Netplay Protocol v1

Wire protocol for networked multiplayer: a lobby/relay server assigns cabinet
slots (0–7), manages the lobby, and relays 40-byte car-state packets between
players at ~60 Hz. This document is the contract for the production backend.
The reference implementation is the Rust server in `raverace/server/` (`rrn1-server`); the game also carries a built-in host (`src/rr_netd.c`).

## 1. Transport and conventions

- UDP, default port **27750**.
- All multi-byte integers are **little-endian**.
- One message per datagram; no fragmentation. The largest legal message
  (WELCOME with a full 8-player roster) is 169 bytes, so every message fits
  in a single datagram comfortably.
- `u8`/`u16`/`u32` = unsigned 8/16/32-bit integer. `char[n]` = n raw bytes.
- Names are printable UTF-8, max 16 bytes. Senders MUST NOT exceed 16 bytes;
  receivers MUST truncate longer names and SHOULD strip control characters.
- A client is identified by its UDP (IP, port) pair for the duration of its
  lobby membership.

## 2. Common header (10 bytes)

Every message starts with:

| Offset | Size | Field  | Value                              |
|--------|------|--------|------------------------------------|
| 0      | 4    | magic  | ASCII `"RRN1"` (0x52 52 4E 31)     |
| 4      | 1    | type   | message type (see §3)              |
| 5      | 1    | flags  | reserved, MUST be 0; receivers ignore |
| 6      | 2    | seq    | sequence number (see §5)           |
| 8      | 2    | len    | payload length in bytes            |
| 10     | len  | payload| len bytes                          |

Receivers MUST silently drop datagrams with a bad magic, or whose `len` does
not match the actual payload size.

**seq semantics:** `seq` is per-sender and increments (mod 2^16) with each new
reliable message a sender originates. Retransmissions reuse the same `seq`.
For unreliable message types (ROSTER, LEAVE, FRAME, PING, PONG) `seq` is
unused and SHOULD be sent as 0; receivers MUST ignore it.

## 3. Message types

| Type  | Name    | Direction      | Reliable |
|-------|---------|----------------|----------|
| 0x01  | HELLO   | client→server  | yes      |
| 0x02  | WELCOME | server→client  | no (answers HELLO) |
| 0x03  | ROSTER  | server→clients | no (self-correcting) |
| 0x04  | READY   | client→server  | yes      |
| 0x05  | START   | client→server  | yes      |
| 0x06  | GO      | server→clients | yes (acked by ACK) |
| 0x07  | LEAVE   | client→server  | no       |
| 0x08  | FRAME   | client→server→clients | no |
| 0x09  | PING    | client→server  | no       |
| 0x0A  | PONG    | server→client  | no       |
| 0x0B  | ACK     | either         | no       |
| 0x0C  | DISCOVER | client→LAN broadcast | no  |
| 0x0D  | ANNOUNCE | host→client   | no (answers DISCOVER) |

### Shared sub-structure: roster

`roster` appears in WELCOME, ROSTER and GO:

```
roster := u8 count; then count entries of:
    u8 slot; u8 ready (0/1); u8 name_len; char name[name_len]
```

Entries are ordered by slot ascending. `count` ≤ 8.

### 0x01 HELLO (client→server, reliable)

Requests lobby entry.

| Offset | Size | Field    | Value                          |
|--------|------|----------|--------------------------------|
| 0      | 1    | name_len | 0–16                           |
| 1      | n    | name     | printable UTF-8, ≤ 16 bytes    |

### 0x02 WELCOME (server→client, answers/acks HELLO)

Header `seq` MUST equal the `seq` of the HELLO it answers, so the client can
match it against its retransmission.

| Offset | Size | Field      | Value                                   |
|--------|------|------------|-----------------------------------------|
| 0      | 1    | your_slot  | 0–7 assigned slot, or 0xFF = rejected   |
| 1      | 1    | state      | 0 = lobby, 1 = race started             |
| 2      | 4    | session_id | current session id (0 while in lobby)   |
| 6      | …    | roster     | current roster                          |

Rejection encoding: `your_slot = 0xFF` with `state = 0` means **lobby full**;
`your_slot = 0xFF` with `state = 1` means **race in progress** — the server
rejects joins while a race is running (see §6); the client should retry later.

### 0x03 ROSTER (server→clients, broadcast)

Broadcast to every lobby member on any lobby change (join, leave, ready flip,
timeout drop). Unreliable but self-correcting: it always carries the full
current state, so a lost ROSTER is healed by the next one.

| Offset | Size | Field      | Value                                 |
|--------|------|------------|---------------------------------------|
| 0      | 1    | state      | 0 = lobby, 1 = race started           |
| 1      | 4    | session_id | current session id (0 while in lobby) |
| 5      | …    | roster     | current roster                        |

### 0x04 READY (client→server, reliable)

| Offset | Size | Field | Value   |
|--------|------|-------|---------|
| 0      | 1    | ready | 0 or 1  |

The server applies the flag to the sender's slot and broadcasts ROSTER. The
ROSTER reflecting the sender's own new flag value is the acknowledgment.

### 0x05 START (client→server, reliable)

Empty payload. Any lobby member may request start. The server responds by
sending GO to every member; GO is the acknowledgment.

### 0x06 GO (server→clients, reliable, acked by ACK)

| Offset | Size | Field      | Value                        |
|--------|------|------------|------------------------------|
| 0      | 4    | session_id | id of the race session       |
| 4      | …    | roster     | final slot assignments       |

Sent individually to each member, each with its own server-chosen `seq`.
Slot assignments in this roster are final for the session. The recipient
answers with ACK carrying the GO's `seq`.

### 0x07 LEAVE (client→server, unreliable)

Empty payload. The server frees the sender's slot and broadcasts ROSTER.
Because LEAVE is unreliable, the liveness timeout (§5) is the ultimate
backstop for departed clients.

### 0x08 FRAME (client→server, relayed to every other slot in the session)

| Offset | Size | Field     | Value                              |
|--------|------|-----------|------------------------------------|
| 0      | 4    | session_id| must match the current session     |
| 4      | 4    | frame_seq | per-sender monotone frame counter  |
| 8      | 1    | cab_id    | sender's slot (= cabinet number)   |
| 9      | 40   | packet    | 40-byte car-state packet, opaque   |

Total datagram size: 10 (header) + 49 = 59 bytes.

The 40-byte `packet` field is the sender's 38-byte game link payload
(`rr_link_pkt_t.data`, the staged C139 packet) followed by 2 zero bytes;
senders MUST zero-fill them and receivers MUST use only the first 38.

The server relays the message **verbatim** (header included) to every *other*
member of the same session. It validates only that the sender holds a slot,
the session_id matches the active session, and `cab_id` equals the sender's
slot; it does NOT inspect `frame_seq` or the 40-byte payload.

The receiver drops frames with a wrong session_id, and drops frames whose
`frame_seq <= ` the last frame_seq seen from that `cab_id` (per-sender
dedup/reordering guard; comparison is plain integer comparison, no wrap
handling — a race does not produce 2^32 frames).

### 0x09 PING (client→server)

| Offset | Size | Field          | Value                |
|--------|------|----------------|----------------------|
| 0      | 4    | client_time_ms | sender's clock, ms   |

### 0x0A PONG (server→client)

| Offset | Size | Field          | Value                     |
|--------|------|----------------|---------------------------|
| 0      | 4    | client_time_ms | echoed verbatim from PING |

RTT = now − echoed value. PING/PONG also serve as liveness traffic.

### 0x0B ACK (either direction)

| Offset | Size | Field     | Value                        |
|--------|------|-----------|------------------------------|
| 0      | 2    | acked_seq | `seq` of the message acked   |

Currently only used to acknowledge GO. Reserved for future reliable types.

### 0x0C DISCOVER (client→LAN, unreliable) / 0x0D ANNOUNCE (host→client)

LAN discovery. A client broadcasts DISCOVER (empty payload) to UDP port 27750
(255.255.255.255, each interface's directed broadcast, and loopback); every
server answers the sender with ANNOUNCE:

| Offset | Size | Field    | Value                               |
|--------|------|----------|-------------------------------------|
| 0      | 1    | state    | 0 = lobby, 1 = race started        |
| 1      | 1    | players  | current member count                |
| 2      | 1    | name_len | 0–16                                |
| 3      | n    | name     | the host's display name             |

The source address of the ANNOUNCE is the server address to join. The game's
built-in host (`src/rr_netd.c`, menu: Online > Host a LAN game) implements the
whole protocol including this; `rrn1-server` answers it too. FRAME
datagrams must be exactly 59 bytes: servers drop any other length.

## 4. Lobby rules

- 8 slots, numbered 0–7. On join the server assigns the **lowest free** slot.
- Slot assignment becomes final at GO (the GO roster is authoritative).
- `session_id` is a server-generated nonzero u32 identifying one race
  session; it is 0 while the server is in lobby state.
- While `state = 1` (race in progress) the server **rejects** new joins with
  WELCOME (`your_slot = 0xFF, state = 1`). It does not hold latecomers for the
  next session; they must re-HELLO after the session ends.
- The session ends when its last member leaves or times out; the server then
  returns to lobby state (`state = 0`, `session_id = 0`) and any remaining
  state for the session is discarded.

## 5. Reliability and liveness

- **Client retransmission:** HELLO, READY and START are resent every
  **500 ms** with the *same* `seq` until the corresponding server response
  arrives:
  - HELLO → WELCOME (matched by header `seq`),
  - READY → any ROSTER reflecting the new ready flag for the client's slot,
  - START → GO.
- **Server retransmission:** GO is resent every **500 ms** to each recipient
  until that recipient's ACK arrives or the recipient is dropped by the
  liveness timeout.
- **Idempotency:** duplicate HELLO from an address that already holds a slot
  MUST be answered with a fresh WELCOME for the existing slot (no second
  join). Duplicate READY/START/ACK MUST be harmless.
- **Liveness timeout:** a client from which the server has received no
  message of any kind for **> 5 s** is dropped: its slot is freed and a
  ROSTER is broadcast. (This also terminates un-ACKed GO retransmission.)
- **Keepalive:** clients send PING at **1 Hz** while idle in the lobby.
  During a race, FRAME traffic doubles as liveness; a player with no car to
  report (paused, crashed out) should fall back to PING at 1 Hz.

## 6. Session lifecycle

```
client                                server
  |--- HELLO (retry 500ms) ---------->|  assign lowest free slot
  |<-- WELCOME (slot, roster) --------|  broadcast ROSTER to all
  |--- READY 1 (retry 500ms) -------->|
  |<-- ROSTER (self-correcting) ------|
  |--- START (retry 500ms) ---------->|
  |<-- GO (session_id, final roster)--|  state = 1, resend GO until ACK
  |--- ACK ---------------------------->|
  |         ... race ...              |
  |--- FRAME ------------------------>|  relay verbatim to other slots
  |<--------------- FRAME ------------|
  |--- LEAVE ------------------------>|  free slot, ROSTER; last member
  |                                   |  out => back to lobby state
```

## 7. Client behavior expectations

- The client's cabinet number in-game **is** `your_slot` from WELCOME/GO.
- On GO the client arms the race; the arcade game's own link-join window
  synchronizes the actual green light, so GO does not need sub-frame
  precision. FRAME sending starts once the race begins.
- FRAMEs are sent at ~60 Hz while racing, `frame_seq` starting at 0 and
  incrementing by 1 per frame.
- A rejected client (`your_slot = 0xFF`) must not enter the lobby retry loop
  faster than one HELLO per second.

## 8. Backend implementor notes

- This document is the contract; the reference server is `raverace/server/`
  (Rust, std only: `cargo build --release`, then `rrn1-server [--port 27750]
  [--bind ADDR] [--name NAME] [--max-per-ip N] [--quiet]`; `cargo test` runs its
  conformance tests -- join, roster, GO retransmission, relay, drops, liveness).
  It is hardened beyond the contract: exact 59-byte FRAMEs, per-IP rate limit on
  addresses that hold no slot, short replies to rejected HELLOs, a per-IP slot cap.
- Client identity is the UDP source address. NAT rebinding mid-session
  looks like a new client; that is acceptable for v1.
- Worst-case message sizes: WELCOME = 10 + 6 + (1 + 8×19) = **169 bytes**,
  GO = 10 + 4 + (1 + 8×19) = **167 bytes**, FRAME = 59 bytes. Nothing in
  v1 exceeds 512 bytes — no MTU concerns.
- The server needs no per-sender frame_seq state and no knowledge of the
  40-byte car-state payload layout; both are owned by the game client.
- All timeouts/intervals (500 ms retry, 5 s liveness, 1 Hz ping) are part of
  the contract; a stricter server breaks conforming clients behind lossy
  links.
- `flags` and the ACK-for-other-types cases are reserved for v2 (e.g.
  reliable FRAME snapshots, host migration). Receivers must ignore unknown
  flags bits and unknown message types rather than erroring.
