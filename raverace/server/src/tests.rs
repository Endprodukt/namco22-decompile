use super::*;
use std::net::{Ipv4Addr, SocketAddrV4};

fn addr(ip: u8, port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, ip), port))
}
fn hello(name: &str) -> Vec<u8> {
    let mut p = vec![name.len() as u8];
    p.extend_from_slice(name.as_bytes());
    msg(ty::HELLO, 7, &p)
}
fn frame(session: u32, fseq: u32, cab: u8) -> Vec<u8> {
    let mut p = session.to_le_bytes().to_vec();
    p.extend_from_slice(&fseq.to_le_bytes());
    p.push(cab);
    p.extend_from_slice(&[0xAB; 40]);
    msg(ty::FRAME, 0, &p)
}
struct T {
    s: Server,
    now: Instant,
}
impl T {
    fn new() -> T {
        let now = Instant::now();
        T { s: Server::new(Config { unknown_pps: 1000, ..Config::default() }, now, 12345), now }
    }
    fn send(&mut self, from: SocketAddr, d: &[u8]) -> Out {
        let mut o = Out::new();
        self.s.handle(d, from, self.now, &mut o);
        o
    }
    fn advance(&mut self, ms: u64) -> Out {
        self.now += Duration::from_millis(ms);
        let mut o = Out::new();
        self.s.tick(self.now, &mut o);
        o
    }
}
fn kinds(o: &Out) -> Vec<u8> {
    o.iter().map(|(_, d)| d[4]).collect()
}
fn to(o: &Out, a: SocketAddr) -> Vec<&Vec<u8>> {
    o.iter().filter(|(t, _)| *t == a).map(|(_, d)| d).collect()
}
/// two clients in a started race; returns their addrs and the session id
fn race() -> (T, SocketAddr, SocketAddr, u32) {
    let mut t = T::new();
    let (a, b) = (addr(1, 5000), addr(2, 5000));
    t.send(a, &hello("A"));
    t.send(b, &hello("B"));
    let o = t.send(a, &msg(ty::START, 9, &[]));
    let go_a = to(&o, a).into_iter().find(|d| d[4] == ty::GO).unwrap().clone();
    let sid = le32(&go_a[10..14]);
    let go_b = to(&o, b).into_iter().find(|d| d[4] == ty::GO).unwrap().clone();
    t.send(a, &msg(ty::ACK, 0, &go_a[6..8]));
    t.send(b, &msg(ty::ACK, 0, &go_b[6..8]));
    (t, a, b, sid)
}

#[test]
fn join_assigns_lowest_free_slot_and_welcome_echoes_hello_seq() {
    let mut t = T::new();
    let o = t.send(addr(1, 1), &hello("ALPHA"));
    let w = to(&o, addr(1, 1)).into_iter().find(|d| d[4] == ty::WELCOME).unwrap();
    assert_eq!(le16(&w[6..8]), 7, "WELCOME must carry the HELLO's seq");
    assert_eq!(w[10], 0, "first slot");
    let o = t.send(addr(2, 1), &hello("BETA"));
    let w = to(&o, addr(2, 1)).into_iter().find(|d| d[4] == ty::WELCOME).unwrap();
    assert_eq!(w[10], 1);
    assert_eq!(kinds(&o).iter().filter(|k| **k == ty::ROSTER).count(), 2, "ROSTER to both members");
    // slot 0 leaves, the next joiner takes it
    t.send(addr(1, 1), &msg(ty::LEAVE, 0, &[]));
    let o = t.send(addr(3, 1), &hello("GAMMA"));
    let w = to(&o, addr(3, 1)).into_iter().find(|d| d[4] == ty::WELCOME).unwrap();
    assert_eq!(w[10], 0);
}

#[test]
fn duplicate_hello_is_answered_without_a_second_join() {
    let mut t = T::new();
    t.send(addr(1, 1), &hello("A"));
    let o = t.send(addr(1, 1), &hello("A"));
    assert_eq!(kinds(&o), vec![ty::WELCOME]);
    assert_eq!(t.s.players(), 1);
}

#[test]
fn lobby_full_and_race_in_progress_reject_with_short_reply() {
    let mut t = T::new();
    t.s.cfg.max_per_ip = 100;
    for i in 0..8 {
        t.send(addr(1, 100 + i), &hello("X"));
    }
    let o = t.send(addr(9, 1), &hello("LATE"));
    let w = &to(&o, addr(9, 1))[0];
    assert_eq!((w[10], w[11]), (0xFF, 0), "lobby full");
    assert!(w.len() <= 18, "a rejection must not carry a roster (amplification)");
    t.send(addr(1, 100), &msg(ty::START, 0, &[]));
    let o = t.send(addr(9, 1), &hello("LATE"));
    let w = &to(&o, addr(9, 1))[0];
    assert_eq!((w[10], w[11]), (0xFF, 1), "race in progress");
}

#[test]
fn per_ip_slot_cap() {
    let mut t = T::new();
    for i in 0..4 {
        t.send(addr(1, 100 + i), &hello("X"));
    }
    let o = t.send(addr(1, 200), &hello("X"));
    assert_eq!(to(&o, addr(1, 200))[0][10], 0xFF);
    assert_eq!(t.s.players(), 4);
}

#[test]
fn ready_flips_broadcast_roster_only_in_lobby() {
    let mut t = T::new();
    t.send(addr(1, 1), &hello("A"));
    let o = t.send(addr(1, 1), &msg(ty::READY, 3, &[1]));
    assert_eq!(kinds(&o), vec![ty::ROSTER]);
    assert_eq!(o[0].1[10 + 5 + 1 + 1], 1, "ready flag in the roster entry");
    t.send(addr(1, 1), &msg(ty::START, 0, &[]));
    let o = t.send(addr(1, 1), &msg(ty::READY, 3, &[0]));
    assert!(o.is_empty());
}

#[test]
fn start_sends_go_to_all_and_resends_until_acked() {
    let mut t = T::new();
    let (a, b) = (addr(1, 1), addr(2, 1));
    t.send(a, &hello("A"));
    t.send(b, &hello("B"));
    let o = t.send(b, &msg(ty::START, 1, &[]));
    assert_eq!(kinds(&o).iter().filter(|k| **k == ty::GO).count(), 2);
    let go_a = to(&o, a).into_iter().find(|d| d[4] == ty::GO).unwrap().clone();
    assert!(t.advance(100).is_empty(), "no resend before 500 ms");
    let o = t.advance(450);
    assert_eq!(kinds(&o).iter().filter(|k| **k == ty::GO).count(), 2, "both un-ACKed");
    t.send(a, &msg(ty::ACK, 0, &go_a[6..8]));
    let o = t.advance(600);
    assert_eq!(kinds(&o).iter().filter(|k| **k == ty::GO).count(), 1, "only b still un-ACKed");
    // a wrong seq does not ack
    t.send(b, &msg(ty::ACK, 0, &[0xFF, 0x7F]));
    assert_eq!(kinds(&t.advance(600)).iter().filter(|k| **k == ty::GO).count(), 1);
}

#[test]
fn frames_are_relayed_verbatim_to_others_only() {
    let (mut t, a, b, sid) = race();
    let f = frame(sid, 0, 0);
    let o = t.send(a, &f);
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].0, b);
    assert_eq!(o[0].1, f, "relayed verbatim, header included");
}

#[test]
fn bad_frames_are_dropped() {
    let (mut t, a, b, sid) = race();
    assert!(t.send(a, &frame(sid ^ 1, 0, 0)).is_empty(), "wrong session");
    assert!(t.send(a, &frame(sid, 0, 1)).is_empty(), "cab_id != slot");
    assert!(t.send(b, &frame(sid, 0, 0)).is_empty(), "cab_id != slot");
    assert!(t.send(addr(9, 9), &frame(sid, 0, 0)).is_empty(), "not a member");
    let mut long = frame(sid, 0, 0);
    long.extend_from_slice(&[0; 20]);
    let l = (long.len() - 10) as u16;
    long[8..10].copy_from_slice(&l.to_le_bytes());
    assert!(t.send(a, &long).is_empty(), "oversize FRAME must not be relayed");
    let short = msg(ty::FRAME, 0, &frame(sid, 0, 0)[10..40]);
    assert!(t.send(a, &short).is_empty(), "short FRAME");
}

#[test]
fn malformed_datagrams_are_ignored() {
    let mut t = T::new();
    assert!(t.send(addr(1, 1), b"short").is_empty());
    let mut bad = hello("A");
    bad[0] = b'X';
    assert!(t.send(addr(1, 1), &bad).is_empty(), "bad magic");
    let mut bad = hello("A");
    bad[8] = 99;
    assert!(t.send(addr(1, 1), &bad).is_empty(), "len mismatch");
    assert_eq!(t.s.players(), 0);
    assert!(t.send(addr(1, 1), &msg(0x7E, 0, &[])).is_empty(), "unknown type");
}

#[test]
fn names_are_cleaned_and_truncated() {
    let mut t = T::new();
    let mut p = vec![20u8];
    p.extend_from_slice(b"AB\x01\x02CDEFGHIJKLMNOPQRS");
    let o = t.send(addr(1, 1), &msg(ty::HELLO, 1, &p));
    let w = &to(&o, addr(1, 1))[0];
    let nl = w[10 + 6 + 1 + 2] as usize;
    let name = &w[10 + 6 + 1 + 3..10 + 6 + 1 + 3 + nl];
    assert!(nl <= 16);
    assert!(name.iter().all(|c| *c >= 0x20));
}

#[test]
fn liveness_timeout_frees_slot_and_ends_session() {
    let (mut t, a, _b, sid) = race();
    // b goes silent; a keeps pinging
    for _ in 0..7 {
        t.now += Duration::from_secs(1);
        let mut o = Out::new();
        t.s.handle(&msg(ty::PING, 0, &[1, 2, 3, 4]), a, t.now, &mut o);
        assert_eq!(kinds(&o), vec![ty::PONG]);
        t.s.tick(t.now, &mut o);
    }
    assert_eq!(t.s.players(), 1, "silent member dropped after 5 s");
    assert!(t.s.in_race());
    t.send(a, &msg(ty::LEAVE, 0, &[]));
    t.advance(100);
    assert!(!t.s.in_race(), "last member out: back to the lobby");
    assert_eq!(t.s.session_id(), 0);
    let _ = sid;
}

#[test]
fn ping_echoes_and_discover_announces() {
    let mut t = T::new();
    let o = t.send(addr(5, 5), &msg(ty::PING, 0, &[9, 8, 7, 6]));
    assert_eq!(o[0].1[10..14], [9, 8, 7, 6]);
    t.send(addr(1, 1), &hello("A"));
    let o = t.send(addr(5, 5), &msg(ty::DISCOVER, 0, &[]));
    let d = &o[0].1;
    assert_eq!(d[4], ty::ANNOUNCE);
    assert_eq!((d[10], d[11]), (0, 1), "state lobby, one player");
    assert_eq!(&d[13..13 + d[12] as usize], b"RRN1 SERVER");
}

#[test]
fn unknown_sources_are_rate_limited_members_are_not() {
    let now = Instant::now();
    let mut s = Server::new(Config { unknown_pps: 5, ..Config::default() }, now, 1);
    let mut o = Out::new();
    for _ in 0..50 {
        s.handle(&msg(ty::DISCOVER, 0, &[]), addr(7, 7), now, &mut o);
    }
    assert_eq!(o.len(), 5, "only the first 5 DISCOVERs in a second are answered");
    s.handle(&hello("A"), addr(1, 1), now + Duration::from_secs(2), &mut o);
    o.clear();
    for _ in 0..50 {
        s.handle(&msg(ty::PING, 0, &[0; 4]), addr(1, 1), now + Duration::from_secs(2), &mut o);
    }
    assert_eq!(o.len(), 50, "a member is not limited");
}
