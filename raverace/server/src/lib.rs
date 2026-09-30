//! RRN1 (Rave Racer Netplay v1) lobby + relay server core -- `../NETPLAY.md` is the contract.
//!
//! The core is transport-free: [`Server::handle`] takes one datagram and appends the datagrams to
//! send to an output list, [`Server::tick`] does the timers. `main.rs` binds a UDP socket around it;
//! the tests drive it directly with a fake clock.
//!
//! Beyond the reference behaviour it is hardened for a public address: FRAMEs must be exactly 59
//! bytes, datagrams from addresses that hold no slot are rate limited per IP, a rejected HELLO is
//! answered with a short reply (no roster), and one IP may hold only `max_per_ip` slots.

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
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Shown to LAN discovery (max 16 bytes on the wire).
    pub name: String,
    /// Slots one IP address may hold at once (several instances on one machine are fine for testing).
    pub max_per_ip: usize,
    /// Datagrams per second accepted from an address that holds no slot.
    pub unknown_pps: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config { name: "RRN1 SERVER".into(), max_per_ip: 4, unknown_pps: 30 }
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
            }
            ty::PING => {
                if p.len() >= 4 {
                    out.push((from, msg(ty::PONG, 0, &p[..4])));
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
                self.state = 0;
                self.session_id = 0;
                self.events.push(format!("session over, relayed {} frames; back to lobby", self.relayed));
                self.relayed = 0;
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

#[cfg(test)]
mod tests;
