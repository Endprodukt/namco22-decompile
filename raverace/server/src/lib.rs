//! RRN1 (Rave Racer Netplay v1) lobby + relay server core -- `../NETPLAY.md` is the contract.
//!
//! The core is transport-free: [`Server::handle`] takes one datagram and appends the datagrams to
//! send to an output list, [`Server::tick`] does the timers. `main.rs` binds a UDP socket around it;
//! the tests drive it directly with a fake clock.
//!
//! Beyond the reference behaviour it is hardened for a public address: FRAMEs must be exactly 59
//! bytes, datagrams from addresses that hold no slot are rate limited per IP, a rejected HELLO is
//! answered with a short reply (no roster), and one IP may hold only `max_per_ip` slots.
//!
//! Robustness: a race session ends by itself (back to the lobby, ROSTER state 0 to the members) when
//! the last player leaves, when no FRAME has arrived for `frame_idle`, or after `max_session`; a PING
//! from an address that holds no slot is answered with the short rejection WELCOME, which is how a
//! client learns that the server restarted or dropped it. CHAT (0x0E) is relayed to the lobby, rate limited.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

pub const MAGIC: &[u8; 4] = b"RRN1";
pub const N_SLOTS: usize = 8;
pub const DEFAULT_PORT: u16 = 27750;
pub const LIVENESS: Duration = Duration::from_secs(5);
pub const GO_RESEND: Duration = Duration::from_millis(500);
/// A FRAME payload: session_id(4) frame_seq(4) cab_id(1) packet(40).
pub const FRAME_PAYLOAD: usize = 49;
/// Longest chat text in bytes.
pub const MAX_CHAT: usize = 96;

pub mod ty {
    pub const HELLO: u8 = 0x01;
    pub const WELCOME: u8 = 0x02;
    pub const ROSTER: u8 = 0x03;
    pub const READY: u8 = 0x04;
    pub const START: u8 = 0x05;
    pub const GO: u8 = 0x06;
    pub const LEAVE: u8 = 0x07;
    pub const FRAME: u8 = 0x08;
    pub const PING: u8 = 0x09;
    pub const PONG: u8 = 0x0A;
    pub const ACK: u8 = 0x0B;
    pub const DISCOVER: u8 = 0x0C;
    pub const ANNOUNCE: u8 = 0x0D;
    pub const CHAT: u8 = 0x0E;
    pub const ROOMS: u8 = 0x0F; // client -> server: list the rooms
    pub const ROOMLIST: u8 = 0x10; // server -> client
    pub const RENAME: u8 = 0x11; // client -> server: change my name (payload as HELLO's name)
    pub const DELROOM: u8 = 0x12; // client -> server: delete the (empty, not racing) room with this id
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Shown to LAN discovery (max 16 bytes on the wire).
    pub name: String,
    /// Slots one IP address may hold at once (several instances on one machine are fine for testing).
    pub max_per_ip: usize,
    /// Datagrams per second accepted from an address that holds no slot.
    pub unknown_pps: u32,
    /// A race session older than this is ended (the players are put back in the lobby).
    pub max_session: Duration,
    /// A race session in which no FRAME arrives for this long is ended.
    pub frame_idle: Duration,
    /// An empty room stays listed (and joinable) this long after its last player left, then closes.
    pub room_ttl: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            name: "RRN1 SERVER".into(),
            max_per_ip: 8,
            unknown_pps: 30,
            max_session: Duration::from_secs(15 * 60),
            frame_idle: Duration::from_secs(90),
            room_ttl: Duration::from_secs(10 * 60),
        }
    }
}

struct Slot {
    addr: SocketAddr,
    name: String,
    ready: bool,
    last_seen: Instant,
    go_seq: u16,
    go_acked: bool,
    last_go: Instant,
    chat_t: Instant,
    chat_n: u32,
}

/// (destination, datagram)
pub type Out = Vec<(SocketAddr, Vec<u8>)>;

pub struct Server {
    cfg: Config,
    slots: [Option<Slot>; N_SLOTS],
    state: u8, // 0 lobby, 1 race
    session_id: u32,
    seq: u16,
    relayed: u64,
    rng: u64,
    limiter: HashMap<IpAddr, (Instant, u32)>,
    last_gc: Instant,
    session_start: Instant,
    last_frame: Instant,
    /// Human-readable events (joins, leaves, session start/end) for the binary to print.
    pub events: Vec<String>,
}

fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn msg(kind: u8, seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(10 + payload.len());
    v.extend_from_slice(MAGIC);
    v.push(kind);
    v.push(0);
    v.extend_from_slice(&seq.to_le_bytes());
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

/// Chat text: printable, at most MAX_CHAT bytes (cut at a character boundary), trimmed.
fn clean_text(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    let mut out = String::new();
    for c in s.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > MAX_CHAT {
            break;
        }
        out.push(c);
    }
    out.trim().to_string()
}

/// A room name: printable, trimmed, at most 24 bytes (cut at a character boundary); empty = none given.
fn clean_label(raw: &[u8]) -> Option<String> {
    let t: String = String::from_utf8_lossy(raw).chars().filter(|c| !c.is_control()).collect();
    let mut t = t.trim().to_string();
    while t.len() > 24 {
        t.pop();
    }
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn clean_name(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    s.chars().filter(|c| !c.is_control()).collect::<String>().chars().take(16).collect()
}

impl Server {
    pub fn new(cfg: Config, now: Instant, seed: u64) -> Self {
        Server {
            cfg,
            slots: std::array::from_fn(|_| None),
            state: 0,
            session_id: 0,
            seq: 0,
            relayed: 0,
            rng: seed | 1,
            limiter: HashMap::new(),
            last_gc: now,
            session_start: now,
            last_frame: now,
            events: Vec::new(),
        }
    }

    pub fn players(&self) -> usize {
        self.slots.iter().flatten().count()
    }
    pub fn in_race(&self) -> bool {
        self.state == 1
    }
    pub fn session_id(&self) -> u32 {
        self.session_id
    }

    fn rand(&mut self) -> u64 {
        // xorshift64*: session ids need to be unpredictable enough, not cryptographic
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn next_seq(&mut self) -> u16 {
        self.seq = self.seq.wrapping_add(1);
        if self.seq == 0 {
            self.seq = 1;
        }
        self.seq
    }

    fn slot_of(&self, a: SocketAddr) -> Option<usize> {
        self.slots.iter().position(|s| s.as_ref().map_or(false, |s| s.addr == a))
    }

    fn roster_payload(&self) -> Vec<u8> {
        let members: Vec<(usize, &Slot)> =
            self.slots.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|s| (i, s))).collect();
        let mut v = vec![members.len() as u8];
        for (i, s) in members {
            let n = s.name.as_bytes();
            v.extend_from_slice(&[i as u8, s.ready as u8, n.len() as u8]);
            v.extend_from_slice(n);
        }
        v
    }

    fn roster_changed(&self, out: &mut Out) {
        let mut p = vec![self.state];
        p.extend_from_slice(&self.session_id.to_le_bytes());
        p.extend_from_slice(&self.roster_payload());
        let m = msg(ty::ROSTER, 0, &p);
        for s in self.slots.iter().flatten() {
            out.push((s.addr, m.clone()));
        }
    }

    fn welcome(&self, to: SocketAddr, slot: Option<usize>, hello_seq: u16, out: &mut Out) {
        let mut p = vec![slot.map_or(0xFF, |s| s as u8), self.state];
        p.extend_from_slice(&self.session_id.to_le_bytes());
        if slot.is_some() {
            p.extend_from_slice(&self.roster_payload());
        } else {
            p.push(0); // a rejection carries an empty roster: nothing to amplify or leak
        }
        out.push((to, msg(ty::WELCOME, hello_seq, &p)));
    }

    fn send_go(&mut self, i: usize, now: Instant, out: &mut Out) {
        if self.slots[i].as_ref().map_or(false, |s| s.go_seq == 0) {
            let q = self.next_seq();
            self.slots[i].as_mut().unwrap().go_seq = q;
        }
        let mut p = self.session_id.to_le_bytes().to_vec();
        p.extend_from_slice(&self.roster_payload());
        let s = self.slots[i].as_mut().unwrap();
        out.push((s.addr, msg(ty::GO, s.go_seq, &p)));
        s.last_go = now;
    }

    /// Per-IP budget for addresses that hold no slot. True = accept.
    fn allow_unknown(&mut self, ip: IpAddr, now: Instant) -> bool {
        if now.duration_since(self.last_gc) > Duration::from_secs(10) {
            self.limiter.retain(|_, (t, _)| now.duration_since(*t) < Duration::from_secs(2));
            self.last_gc = now;
        }
        if self.limiter.len() > 4096 && !self.limiter.contains_key(&ip) {
            return false; // a flood of distinct sources: shed instead of growing
        }
        let e = self.limiter.entry(ip).or_insert((now, 0));
        if now.duration_since(e.0) >= Duration::from_secs(1) {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= self.cfg.unknown_pps
    }

    /// One received datagram.
    pub fn handle(&mut self, data: &[u8], from: SocketAddr, now: Instant, out: &mut Out) {
        if data.len() < 10 || &data[0..4] != MAGIC {
            return;
        }
        let plen = le16(&data[8..10]) as usize;
        if plen != data.len() - 10 {
            return;
        }
        let kind = data[4];
        let seq = le16(&data[6..8]);
        let p = &data[10..];

        let slot = self.slot_of(from);
        match slot {
            Some(i) => self.slots[i].as_mut().unwrap().last_seen = now,
            None => {
                // only what earns a reply is charged to the per-IP budget: everything else from a
                // stranger (a restarted or ghost client's FRAMEs, stray ACKs) is dropped for free, so
                // it cannot starve the PING that tells that client to rejoin (several players share a NAT)
                if !matches!(kind, ty::HELLO | ty::PING | ty::DISCOVER) {
                    return;
                }
                if !self.allow_unknown(from.ip(), now) {
                    return;
                }
            }
        }

        match kind {
            ty::HELLO => self.on_hello(from, seq, p, slot, now, out),
            ty::READY => {
                if let (Some(i), 0, true) = (slot, self.state, !p.is_empty()) {
                    self.slots[i].as_mut().unwrap().ready = p[0] != 0;
                    self.roster_changed(out);
                }
            }
            ty::START => {
                if slot.is_some() && self.state == 0 {
                    self.state = 1;
                    let mut id = 0u32;
                    while id == 0 {
                        id = (self.rand() >> 16) as u32;
                    }
                    self.session_id = id;
                    self.relayed = 0;
                    self.session_start = now;
                    self.last_frame = now;
                    self.events.push(format!(
                        "race start: session {:08x}, {} player(s)",
                        id,
                        self.players()
                    ));
                    for i in 0..N_SLOTS {
                        if let Some(s) = self.slots[i].as_mut() {
                            s.go_seq = 0;
                            s.go_acked = false;
                        }
                        if self.slots[i].is_some() {
                            self.send_go(i, now, out);
                        }
                    }
                    self.roster_changed(out);
                }
            }
            ty::ACK => {
                if let Some(i) = slot {
                    if p.len() >= 2 {
                        let s = self.slots[i].as_mut().unwrap();
                        if le16(p) == s.go_seq && !s.go_acked {
                            s.go_acked = true;
                        }
                    }
                }
            }
            ty::LEAVE => {
                if let Some(i) = slot {
                    let s = self.slots[i].take().unwrap();
                    self.events.push(format!("leave slot {} '{}'", i, s.name));
                    self.roster_changed(out);
                }
            }
            ty::FRAME => {
                // exactly one legal size: an oversize datagram would be copied to every peer
                if slot.is_none() || self.state != 1 || p.len() != FRAME_PAYLOAD {
                    return;
                }
                if le32(p) != self.session_id || p[8] as usize != slot.unwrap() {
                    return;
                }
                for (i, s) in self.slots.iter().enumerate() {
                    if let Some(s) = s {
                        if Some(i) != slot {
                            out.push((s.addr, data.to_vec()));
                        }
                    }
                }
                self.relayed += 1;
                self.last_frame = now;
            }
            ty::PING => {
                if p.len() >= 4 {
                    if slot.is_some() {
                        out.push((from, msg(ty::PONG, 0, &p[..4])));
                    } else {
                        // not a member (the server restarted, or timed this client out): the short
                        // rejection tells the client to rejoin instead of waiting on a dead slot
                        self.welcome(from, None, 0, out);
                    }
                }
            }
            ty::CHAT => {
                if let Some(i) = slot {
                    self.on_chat(i, p, now, out);
                }
            }
            ty::RENAME => {
                if let (Some(i), false) = (slot, p.is_empty()) {
                    let n = (p[0] as usize).min(16).min(p.len() - 1);
                    let name = clean_name(&p[1..1 + n]);
                    let s = self.slots[i].as_mut().unwrap();
                    if name != s.name {
                        self.events.push(format!("slot {} renamed '{}' -> '{}'", i, s.name, name));
                        s.name = name;
                        self.roster_changed(out);
                    }
                }
            }
            ty::DISCOVER => {
                let name = clean_name(self.cfg.name.as_bytes());
                let mut a = vec![self.state, self.players() as u8, name.len() as u8];
                a.extend_from_slice(name.as_bytes());
                out.push((from, msg(ty::ANNOUNCE, 0, &a)));
            }
            _ => {} // unknown types: ignored (the contract)
        }
    }

    fn on_chat(&mut self, i: usize, p: &[u8], now: Instant, out: &mut Out) {
        let text = clean_text(p);
        if text.is_empty() {
            return;
        }
        {
            let s = self.slots[i].as_mut().unwrap();
            if now.duration_since(s.chat_t) >= Duration::from_secs(2) {
                s.chat_t = now;
                s.chat_n = 0;
            }
            s.chat_n += 1;
            if s.chat_n > 5 {
                return; // 5 lines per 2 s per player
            }
        }
        let mut pl = vec![i as u8];
        pl.extend_from_slice(text.as_bytes());
        let m = msg(ty::CHAT, 0, &pl);
        for s in self.slots.iter().flatten() {
            out.push((s.addr, m.clone())); // the sender gets its own line back: what it sees is what was relayed
        }
    }

    /// Back to the lobby: every member stays, ready flags and GO bookkeeping are cleared.
    fn end_session(&mut self, why: &str, out: &mut Out) {
        self.events.push(format!("session over ({}), relayed {} frames; back to lobby", why, self.relayed));
        self.state = 0;
        self.session_id = 0;
        self.relayed = 0;
        for s in self.slots.iter_mut().flatten() {
            s.ready = false;
            s.go_seq = 0;
            s.go_acked = false;
        }
        self.roster_changed(out);
    }

    fn on_hello(&mut self, from: SocketAddr, seq: u16, p: &[u8], slot: Option<usize>, now: Instant, out: &mut Out) {
        if let Some(i) = slot {
            self.welcome(from, Some(i), seq, out); // duplicate HELLO: re-answer, no second join
            return;
        }
        if p.is_empty() {
            return;
        }
        if self.state == 1 {
            self.welcome(from, None, seq, out); // race in progress
            return;
        }
        let same_ip = self.slots.iter().flatten().filter(|s| s.addr.ip() == from.ip()).count();
        let free = self.slots.iter().position(|s| s.is_none());
        let free = match free {
            Some(f) if same_ip < self.cfg.max_per_ip => f,
            _ => {
                self.events.push(format!("join from {} rejected", from));
                self.welcome(from, None, seq, out);
                return;
            }
        };
        let n = (p[0] as usize).min(16).min(p.len() - 1);
        let name = clean_name(&p[1..1 + n]);
        self.events.push(format!("join slot {} '{}' from {}", free, name, from));
        self.slots[free] = Some(Slot {
            addr: from,
            name,
            ready: false,
            last_seen: now,
            go_seq: 0,
            go_acked: false,
            last_go: now,
            chat_t: now,
            chat_n: 0,
        });
        self.welcome(from, Some(free), seq, out);
        self.roster_changed(out);
    }

    /// Timers: liveness drop, GO retransmission, session end. Call at ~10 Hz or faster.
    pub fn tick(&mut self, now: Instant, out: &mut Out) {
        let mut changed = false;
        for i in 0..N_SLOTS {
            if let Some(s) = &self.slots[i] {
                if now.duration_since(s.last_seen) > LIVENESS {
                    self.events.push(format!("timeout slot {} '{}'", i, s.name));
                    self.slots[i] = None;
                    changed = true;
                }
            }
        }
        if changed {
            self.roster_changed(out);
        }
        if self.state == 1 {
            if self.players() == 0 {
                self.end_session("everyone left", out);
            } else if now.duration_since(self.session_start) > self.cfg.max_session {
                self.end_session("time limit", out);
            } else if now.duration_since(self.last_frame) > self.cfg.frame_idle {
                self.end_session("no traffic", out);
            } else {
                for i in 0..N_SLOTS {
                    let due = self.slots[i]
                        .as_ref()
                        .map_or(false, |s| !s.go_acked && now.duration_since(s.last_go) >= GO_RESEND);
                    if due {
                        self.send_go(i, now, out);
                    }
                }
            }
        }
    }
}

/// Many rooms on one port. Each room is an independent `Server` (its own lobby, roster, race and chat), so one room's race never
/// blocks anyone else. Rooms have stable ids (1..=254). A stranger's HELLO goes to the fullest room still in its lobby (people
/// gather), or opens a new room when every room is racing or full; a HELLO that ends with a room selector byte picks a room (an id)
/// or creates one (255) instead. A known address always reaches its own room. Old clients send no selector and are unchanged.
struct Room {
    id: u8,
    empty_since: Option<Instant>, // when its last player left (None while anyone is in it)
    name: Option<String>, // set by the creator (HELLO with ROOM_NEW + a name); else "<first player>'s room"
    srv: Server,
}

pub struct Hub {
    cfg: Config,
    rooms: Vec<Room>,
    max_rooms: usize,
    next_seed: u64,
    pub events: Vec<String>,
}

/// room selector byte at the end of a HELLO payload
pub const ROOM_NEW: u8 = 255;

impl Hub {
    pub fn new(cfg: Config, max_rooms: usize, now: Instant, seed: u64) -> Hub {
        let first = Room { id: 1, empty_since: Some(now), name: None, srv: Server::new(cfg.clone(), now, seed) };
        Hub { cfg, rooms: vec![first], max_rooms: max_rooms.clamp(1, 20), next_seed: seed, events: Vec::new() }
    }
    pub fn rooms(&self) -> usize {
        self.rooms.len()
    }
    pub fn players(&self) -> usize {
        self.rooms.iter().map(|r| r.srv.players()).sum()
    }
    fn open_room(&mut self, now: Instant, name: Option<String>) -> usize {
        let mut id = 1u8;
        while self.rooms.iter().any(|r| r.id == id) {
            id += 1;
        }
        self.next_seed = self.next_seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.rooms.push(Room { id, empty_since: Some(now), name, srv: Server::new(self.cfg.clone(), now, self.next_seed) });
        self.events.push(format!("room {} opened", id));
        self.rooms.len() - 1
    }
    /// Room for one more? At the limit the oldest EMPTY room is closed to make space (an abandoned room must not block new ones).
    fn make_space(&mut self) -> bool {
        if self.rooms.len() < self.max_rooms {
            return true;
        }
        let oldest = self
            .rooms
            .iter()
            .enumerate()
            .filter(|(_, r)| r.srv.players() == 0 && r.srv.state == 0)
            .min_by_key(|(_, r)| r.empty_since)
            .map(|(i, _)| i);
        match oldest {
            Some(i) => {
                let id = self.rooms.remove(i).id;
                self.events.push(format!("room {} closed (made space)", id));
                true
            }
            None => false,
        }
    }
    /// Can this address take a slot in room `r` right now?
    fn joinable(&self, r: usize, ip: IpAddr) -> bool {
        let s = &self.rooms[r].srv;
        s.state == 0
            && s.players() < N_SLOTS
            && s.slots.iter().flatten().filter(|x| x.addr.ip() == ip).count() < self.cfg.max_per_ip
    }
    /// The fullest joinable room (ties: the lowest index).
    fn pick(&self, ip: IpAddr) -> Option<usize> {
        let mut best: Option<usize> = None;
        for r in 0..self.rooms.len() {
            if self.joinable(r, ip) && best.map_or(true, |b| self.rooms[r].srv.players() > self.rooms[b].srv.players()) {
                best = Some(r);
            }
        }
        best
    }
    fn drain(&mut self, r: usize) {
        let multi = self.rooms.len() > 1;
        let id = self.rooms[r].id;
        let evs: Vec<String> = self.rooms[r].srv.events.drain(..).collect();
        for e in evs {
            self.events.push(if multi { format!("[room {}] {}", id, e) } else { e });
        }
    }
    fn room_name(&self, r: usize) -> String {
        if let Some(n) = &self.rooms[r].name {
            return n.clone();
        }
        match self.rooms[r].srv.slots.iter().flatten().next() {
            Some(s) => format!("{}'s room", s.name),
            None => format!("Room {}", self.rooms[r].id),
        }
    }
    /// ROOMLIST payload: count, then per room: id, state, players, mine, name_len, name
    fn roomlist(&self, to: SocketAddr) -> Vec<u8> {
        let mut p = vec![self.rooms.len().min(20) as u8];
        for r in 0..self.rooms.len().min(20) {
            let room = &self.rooms[r];
            let mut name = self.room_name(r);
            while name.len() > 24 {
                name.pop();
            }
            p.push(room.id);
            p.push(room.srv.state);
            p.push(room.srv.players() as u8);
            p.push(room.srv.slot_of(to).is_some() as u8);
            p.push(name.len() as u8);
            p.extend_from_slice(name.as_bytes());
        }
        msg(ty::ROOMLIST, 0, &p)
    }
    pub fn handle(&mut self, data: &[u8], from: SocketAddr, now: Instant, out: &mut Out) {
        if data.len() < 10 || &data[0..4] != MAGIC || le16(&data[8..10]) as usize != data.len() - 10 {
            return;
        }
        let kind = data[4];
        let seq = le16(&data[6..8]);
        let p = &data[10..];
        let member = self.rooms.iter().position(|r| r.srv.slot_of(from).is_some());

        if kind == ty::DELROOM {
            // only an EMPTY room that is not racing, and never the last one; answered with the fresh room list either way
            if member.is_some() || self.rooms[0].srv.allow_unknown(from.ip(), now) {
                if let Some(&id) = p.first() {
                    if self.rooms.len() > 1 {
                        if let Some(r) = self.rooms.iter().position(|r| r.id == id) {
                            if self.rooms[r].srv.players() == 0 && self.rooms[r].srv.state == 0 {
                                self.rooms.remove(r);
                                self.events.push(format!("room {} deleted", id));
                            }
                        }
                    }
                }
                out.push((from, self.roomlist(from)));
            }
            return;
        }

        if kind == ty::ROOMS {
            if member.is_some() || self.rooms[0].srv.allow_unknown(from.ip(), now) {
                out.push((from, self.roomlist(from)));
            }
            if let Some(r) = member {
                self.rooms[r].srv.handle(data, from, now, out); // keeps the member's liveness fresh
                self.drain(r);
            }
            return;
        }

        let r = if let Some(r) = member {
            r
        } else if kind == ty::HELLO && !p.is_empty() {
            let sel = p.get(1 + p[0] as usize).copied().unwrap_or(0);
            let reject = |h: &mut Hub, out: &mut Out| {
                if h.rooms[0].srv.allow_unknown(from.ip(), now) {
                    h.rooms[0].srv.welcome(from, None, seq, out);
                }
            };
            if sel == ROOM_NEW {
                if self.make_space() {
                    // an optional room name follows the selector: len, bytes
                    let at = 2 + p[0] as usize;
                    let nm = match p.get(at) {
                        Some(&l) if at + 1 + l as usize <= p.len() => clean_label(&p[at + 1..at + 1 + l as usize]),
                        _ => None,
                    };
                    self.open_room(now, nm)
                } else {
                    reject(self, out);
                    return;
                }
            } else if sel != 0 {
                match self.rooms.iter().position(|r| r.id == sel) {
                    Some(r) => r, // the room itself answers: joined, "race in progress" or "lobby full"
                    None => {
                        reject(self, out);
                        return;
                    }
                }
            } else if let Some(r) = self.pick(from.ip()) {
                r
            } else if self.make_space() {
                self.open_room(now, None)
            } else {
                0
            }
        } else if let Some(r) = self.pick(from.ip()) {
            r
        } else {
            0 // a stranger's PING / DISCOVER / junk: any room answers
        };
        self.rooms[r].srv.handle(data, from, now, out);
        self.drain(r);
    }
    pub fn tick(&mut self, now: Instant, out: &mut Out) {
        for r in 0..self.rooms.len() {
            self.rooms[r].srv.tick(now, out);
            self.drain(r);
        }
        for room in self.rooms.iter_mut() {
            let empty = room.srv.players() == 0 && room.srv.state == 0;
            if !empty {
                room.empty_since = None;
            } else if room.empty_since.is_none() {
                room.empty_since = Some(now);
            }
        }
        let mut r = self.rooms.len();
        while r > 0 && self.rooms.len() > 1 {
            r -= 1;
            if let Some(t) = self.rooms[r].empty_since {
                if now.duration_since(t) >= self.cfg.room_ttl {
                    let id = self.rooms.remove(r).id;
                    self.events.push(format!("room {} closed (empty for {} min)", id, self.cfg.room_ttl.as_secs() / 60));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
