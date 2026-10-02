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
    let now = Instant::now();
    let mut t = T { s: Server::new(Config { max_per_ip: 4, unknown_pps: 1000, ..Config::default() }, now, 1), now };
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
    t.send(addr(1, 1), &hello("A"));
    let o = t.send(addr(1, 1), &msg(ty::PING, 0, &[9, 8, 7, 6]));
    assert_eq!(o[0].1[10..14], [9, 8, 7, 6]);
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

// ---- robustness: sessions end by themselves, restarts are noticed, chat ----

fn roster_state(d: &[u8]) -> u8 {
    d[10]
}

/// Let `secs` pass in 1 s steps with both players pinging (alive but not racing), plus a frame from
/// `a` every step when `frames` holds the session id. Returns the datagrams of the LAST step.
fn run(t: &mut T, a: SocketAddr, b: SocketAddr, secs: u64, frames: Option<u32>) -> Out {
    let mut last = Out::new();
    for i in 0..secs {
        t.send(a, &msg(ty::PING, 0, &[1, 2, 3, 4]));
        t.send(b, &msg(ty::PING, 0, &[1, 2, 3, 4]));
        if let Some(sid) = frames {
            t.send(a, &frame(sid, i as u32 + 1, 0));
        }
        last = t.advance(1000);
    }
    last
}

#[test]
fn session_ends_on_time_limit_and_players_stay() {
    let (mut t, a, b, sid) = race();
    // frames keep flowing the whole time: only the time limit can end this one
    let mut o = Out::new();
    for _ in 0..(15 * 60 + 2) {
        o.extend(run(&mut t, a, b, 1, Some(sid)));
        if !t.s.in_race() { break; }
    }
    assert!(!t.s.in_race(), "time limit must end the session");
    assert_eq!(t.s.players(), 2, "players stay connected");
    // both get a ROSTER with state 0 (the client reads that as 'back to the lobby')
    for who in [a, b] {
        let r = to(&o, who).into_iter().find(|d| d[4] == ty::ROSTER).expect("roster");
        assert_eq!(roster_state(r), 0);
    }
}

#[test]
fn session_ends_when_no_frames_arrive_even_if_pings_do() {
    let (mut t, a, b, sid) = race();
    t.send(a, &frame(sid, 1, 0));
    run(&mut t, a, b, 120, None); // pings keep the slots alive, not the race
    assert!(!t.s.in_race(), "an idle race must end");
    assert_eq!(t.s.players(), 2);
}

#[test]
fn frames_keep_a_session_alive() {
    let (mut t, a, b, sid) = race();
    run(&mut t, a, b, 300, Some(sid));
    assert!(t.s.in_race(), "5 minutes of frames is a live race");
}

#[test]
fn a_new_race_can_start_after_an_ended_one() {
    let (mut t, a, b, _sid) = race();
    run(&mut t, a, b, 120, None); // idle: the race ends
    assert!(!t.s.in_race());
    let o = t.send(a, &msg(ty::START, 11, &[]));
    assert!(t.s.in_race());
    assert!(kinds(&o).contains(&ty::GO));
    // and a newcomer is turned away while that race runs, welcome again after it ends
    let c = addr(3, 5000);
    let o = t.send(c, &hello("C"));
    assert_eq!(to(&o, c)[0][10], 0xFF);
}

#[test]
fn ping_from_a_non_member_gets_the_rejection_not_a_pong() {
    let mut t = T::new();
    let ghost = addr(9, 4000);
    let o = t.send(ghost, &msg(ty::PING, 0, &[1, 2, 3, 4]));
    let d = &to(&o, ghost)[0];
    assert_eq!(d[4], ty::WELCOME);
    assert_eq!(d[10], 0xFF);
    assert!(d.len() <= 20, "short: no roster to reflect");
    // a member still gets its PONG
    let a = addr(1, 5000);
    t.send(a, &hello("A"));
    let o = t.send(a, &msg(ty::PING, 0, &[9, 9, 9, 9]));
    assert_eq!(to(&o, a)[0][4], ty::PONG);
}

#[test]
fn restarted_server_forgets_everyone_and_lets_them_rejoin() {
    let (t, a, _b, _sid) = race();
    let mut fresh = T::new(); // the server process restarted
    let o = fresh.send(a, &msg(ty::PING, 0, &[1, 2, 3, 4]));
    assert_eq!(to(&o, a)[0][10], 0xFF);
    let o = fresh.send(a, &hello("A"));
    assert_eq!(to(&o, a)[0][10], 0); // slot 0 again
    drop(t);
}

fn chat(text: &[u8]) -> Vec<u8> {
    msg(ty::CHAT, 0, text)
}

#[test]
fn chat_is_relayed_to_everyone_with_the_senders_slot() {
    let mut t = T::new();
    let (a, b) = (addr(1, 5000), addr(2, 5000));
    t.send(a, &hello("A"));
    t.send(b, &hello("B"));
    let o = t.send(b, &chat(b"hello all"));
    for who in [a, b] {
        let d = to(&o, who).into_iter().find(|d| d[4] == ty::CHAT).expect("chat");
        assert_eq!(d[10], 1); // sender slot
        assert_eq!(&d[11..], b"hello all");
    }
}

#[test]
fn chat_works_during_a_race_and_from_members_only() {
    let (mut t, a, _b, _sid) = race();
    assert!(kinds(&t.send(a, &chat(b"gl hf"))).contains(&ty::CHAT));
    let ghost = addr(9, 1);
    assert!(t.send(ghost, &chat(b"spam")).is_empty());
}

#[test]
fn chat_is_cleaned_truncated_and_rate_limited() {
    let mut t = T::new();
    let a = addr(1, 5000);
    t.send(a, &hello("A"));
    let o = t.send(a, &chat(b"a\x07b\x1b[31mc"));
    let d = to(&o, a)[0];
    assert_eq!(&d[11..], b"ab[31mc", "control characters are stripped");
    assert!(t.send(a, &chat(b"   ")).is_empty(), "blank lines are dropped");
    let long = vec![b'x'; 400];
    let d = to(&t.send(a, &chat(&long)), a)[0].clone();
    assert_eq!(d.len() - 11, MAX_CHAT);
    // 5 lines per 2 s: the 6th within the window is dropped, then it recovers
    let mut sent = 3; // already used 3 above (two relayed + one blank dropped before counting? blank returns early)
    sent -= 1;
    let mut relayed = 0;
    for _ in 0..8 {
        if !t.send(a, &chat(b"spam")).is_empty() {
            relayed += 1;
        }
    }
    assert!(relayed + sent <= 5 + 0, "rate limit holds: {} relayed", relayed + sent);
    t.advance(2500);
    assert!(!t.send(a, &chat(b"back")).is_empty());
}

#[test]
fn a_flood_of_stranger_frames_does_not_starve_its_ping() {
    let mut t = T { s: Server::new(Config::default(), Instant::now(), 1), now: Instant::now() };
    let ghost = addr(9, 4000);
    for i in 0..500 {
        assert!(t.send(ghost, &frame(1, i, 0)).is_empty());
    }
    let o = t.send(ghost, &msg(ty::PING, 0, &[1, 2, 3, 4]));
    assert_eq!(to(&o, ghost)[0][10], 0xFF, "the rejection still gets through");
}

#[test]
fn eight_players_from_one_address_fit_by_default() {
    let mut t = T::new();
    for i in 0..8 {
        let o = t.send(addr(1, 100 + i), &hello("X"));
        assert_eq!(to(&o, addr(1, 100 + i))[0][10], i as u8);
    }
    assert_eq!(t.s.players(), 8);
}

fn hub_t() -> (Hub, Instant) {
    let now = Instant::now();
    (Hub::new(Config { unknown_pps: 1000, ..Config::default() }, 4, now, 99), now)
}
fn hsend(h: &mut Hub, now: Instant, from: SocketAddr, d: &[u8]) -> Out {
    let mut o = Out::new();
    h.handle(d, from, now, &mut o);
    o
}
fn welcome_slot(o: &Out, a: SocketAddr) -> (u8, u8) {
    let w = to(o, a).into_iter().find(|d| d[4] == ty::WELCOME).expect("WELCOME");
    (w[10], w[11])
}

#[test]
fn hub_a_second_room_opens_while_the_first_is_racing() {
    let (mut h, now) = hub_t();
    let (a, b, c) = (addr(1, 5000), addr(2, 5000), addr(3, 5000));
    hsend(&mut h, now, a, &hello("A"));
    hsend(&mut h, now, b, &hello("B"));
    let o = hsend(&mut h, now, a, &msg(ty::START, 9, &[]));
    assert!(to(&o, a).iter().any(|d| d[4] == ty::GO), "room 0 is racing");
    let o = hsend(&mut h, now, c, &hello("C"));
    assert_eq!(welcome_slot(&o, c), (0, 0), "C gets slot 0 of a NEW lobby, not a 'race in progress' rejection");
    assert_eq!(h.rooms(), 2);
}

#[test]
fn hub_newcomers_gather_in_the_fullest_lobby_and_rooms_are_isolated() {
    let (mut h, now) = hub_t();
    let (a, b, c, d) = (addr(1, 1), addr(2, 1), addr(3, 1), addr(4, 1));
    hsend(&mut h, now, a, &hello("A"));
    hsend(&mut h, now, b, &hello("B"));
    hsend(&mut h, now, a, &msg(ty::START, 9, &[])); // room 0 races
    hsend(&mut h, now, c, &hello("C")); // room 1
    let o = hsend(&mut h, now, d, &hello("D"));
    assert_eq!(welcome_slot(&o, d), (1, 0), "D joins C's lobby");
    let o = hsend(&mut h, now, c, &msg(ty::CHAT, 0, b"hi"));
    assert!(to(&o, d).iter().any(|x| x[4] == ty::CHAT), "chat reaches the same room");
    assert!(to(&o, a).is_empty() && to(&o, b).is_empty(), "and never the racing room");
}

#[test]
fn hub_rejects_only_when_every_room_is_used_and_closes_empty_rooms() {
    let (mut h, mut now) = hub_t(); // max 4 rooms
    for i in 0..4u8 {
        let (x, y) = (addr(10 + i * 2, 1), addr(11 + i * 2, 1));
        hsend(&mut h, now, x, &hello("X"));
        hsend(&mut h, now, y, &hello("Y"));
        hsend(&mut h, now, x, &msg(ty::START, 9, &[]));
    }
    assert_eq!(h.rooms(), 4);
    let z = addr(99, 1);
    let o = hsend(&mut h, now, z, &hello("Z"));
    assert_eq!(welcome_slot(&o, z), (0xFF, 1), "all rooms racing, none can open: rejected as before");
    now += Duration::from_secs(30); // everyone times out
    let mut o = Out::new();
    h.tick(now, &mut o);
    h.tick(now, &mut o);
    now += Duration::from_secs(11 * 60); // and the empty rooms outlive their grace period
    h.tick(now, &mut o);
    assert_eq!(h.rooms(), 1, "empty rooms close after the grace period, one stays");
}

fn hello_room(name: &str, room: u8) -> Vec<u8> {
    let mut p = vec![name.len() as u8];
    p.extend_from_slice(name.as_bytes());
    p.push(room);
    msg(ty::HELLO, 7, &p)
}
fn roomlist(o: &Out, a: SocketAddr) -> Vec<(u8, u8, u8, u8, String)> {
    let d = to(o, a).into_iter().find(|d| d[4] == ty::ROOMLIST).expect("ROOMLIST").clone();
    let p = &d[10..];
    let mut v = vec![];
    let mut off = 1;
    for _ in 0..p[0] {
        let nl = p[off + 4] as usize;
        v.push((p[off], p[off + 1], p[off + 2], p[off + 3], String::from_utf8(p[off + 5..off + 5 + nl].to_vec()).unwrap()));
        off += 5 + nl;
    }
    v
}

#[test]
fn hub_lists_rooms_and_new_clients_can_pick_or_create_one() {
    let (mut h, now) = hub_t();
    let (a, b, c, d) = (addr(1, 1), addr(2, 1), addr(3, 1), addr(4, 1));
    hsend(&mut h, now, a, &hello("ALPHA")); // old client: auto, room 1
    let o = hsend(&mut h, now, b, &hello_room("BETA", ROOM_NEW)); // creates room 2
    assert_eq!(welcome_slot(&o, b), (0, 0));
    let o = hsend(&mut h, now, c, &msg(ty::ROOMS, 0, &[])); // a stranger may list
    let l = roomlist(&o, c);
    assert_eq!(l.len(), 2);
    assert_eq!((l[0].0, l[0].2, l[0].3, l[0].4.as_str()), (1, 1, 0, "ALPHA's room"));
    assert_eq!((l[1].0, l[1].2, l[1].4.as_str()), (2, 1, "BETA's room"));
    let o = hsend(&mut h, now, a, &msg(ty::ROOMS, 0, &[]));
    assert_eq!(roomlist(&o, a)[0].3, 1, "the asker's own room is marked");
    let o = hsend(&mut h, now, d, &hello_room("DELTA", 2)); // picks room 2 explicitly
    assert_eq!(welcome_slot(&o, d), (1, 0));
    let o = hsend(&mut h, now, addr(9, 1), &hello_room("GHOST", 77)); // no such room
    assert_eq!(welcome_slot(&o, addr(9, 1)).0, 0xFF);
}

#[test]
fn hub_explicit_join_of_a_racing_room_is_refused_and_leaving_lets_you_switch() {
    let (mut h, now) = hub_t();
    let (a, b, c) = (addr(1, 1), addr(2, 1), addr(3, 1));
    hsend(&mut h, now, a, &hello("A"));
    hsend(&mut h, now, b, &hello("B"));
    hsend(&mut h, now, a, &msg(ty::START, 9, &[])); // room 1 races
    let o = hsend(&mut h, now, c, &hello_room("C", 1));
    assert_eq!(welcome_slot(&o, c), (0xFF, 1), "that room is racing");
    hsend(&mut h, now, c, &hello_room("C", ROOM_NEW));
    hsend(&mut h, now, c, &msg(ty::LEAVE, 0, &[]));
    let o = hsend(&mut h, now, c, &hello_room("C", ROOM_NEW));
    assert_eq!(welcome_slot(&o, c).0, 0, "after leaving, C creates another room");
}

#[test]
fn rename_updates_the_roster() {
    let (mut h, now) = hub_t();
    let (a, b) = (addr(1, 1), addr(2, 1));
    hsend(&mut h, now, a, &hello("OLD"));
    hsend(&mut h, now, b, &hello("BOB"));
    let mut p = vec![3u8];
    p.extend_from_slice(b"NEW");
    let o = hsend(&mut h, now, a, &msg(ty::RENAME, 0, &p));
    let r = to(&o, b).into_iter().find(|d| d[4] == ty::ROSTER).expect("roster to B");
    assert!(r.windows(3).any(|w| w == b"NEW"), "B sees the new name");
    let o = hsend(&mut h, now, addr(5, 1), &msg(ty::RENAME, 0, &p)); // a stranger cannot rename anyone
    assert!(o.is_empty());
}

#[test]
fn a_created_room_can_be_named() {
    let (mut h, now) = hub_t();
    let (a, b) = (addr(1, 1), addr(2, 1));
    let mut p = vec![3u8];
    p.extend_from_slice(b"ANN");
    p.push(ROOM_NEW);
    p.push(11);
    p.extend_from_slice(b"  Night Run");
    hsend(&mut h, now, a, &msg(ty::HELLO, 7, &p));
    let o = hsend(&mut h, now, b, &msg(ty::ROOMS, 0, &[]));
    let l = roomlist(&o, b);
    assert_eq!(l.len(), 2);
    assert_eq!(l[1].4, "Night Run", "trimmed, and used instead of \"ANN's room\"");
    // a truncated name field is ignored, not a crash
    let mut q = vec![3u8];
    q.extend_from_slice(b"BOB");
    q.push(ROOM_NEW);
    q.push(50);
    q.extend_from_slice(b"short");
    hsend(&mut h, now, b, &msg(ty::HELLO, 7, &q));
    let o = hsend(&mut h, now, addr(7, 1), &msg(ty::ROOMS, 0, &[]));
    assert_eq!(roomlist(&o, addr(7, 1))[2].4, "BOB's room");
}

#[test]
fn rooms_outlive_their_creator_so_several_can_be_made() {
    let (mut h, mut now) = hub_t();
    let a = addr(1, 1);
    let mut o = Out::new();
    for name in ["One", "Two", "Three"] {
        let mut p = vec![1u8, b'A', ROOM_NEW, name.len() as u8];
        p.extend_from_slice(name.as_bytes());
        hsend(&mut h, now, a, &msg(ty::HELLO, 7, &p)); // create (leaving the previous one)
        hsend(&mut h, now, a, &msg(ty::LEAVE, 0, &[]));
        now += Duration::from_secs(1);
        h.tick(now, &mut o);
    }
    let l = hsend(&mut h, now, addr(5, 1), &msg(ty::ROOMS, 0, &[]));
    let names: Vec<String> = roomlist(&l, addr(5, 1)).into_iter().map(|r| r.4).collect();
    for n in ["One", "Two", "Three"] {
        assert!(names.iter().any(|x| x == n), "{} must still be listed, got {:?}", n, names);
    }
    now += Duration::from_secs(5 * 60);
    h.tick(now, &mut o);
    assert!(h.rooms() >= 3, "still there after 5 minutes");
    now += Duration::from_secs(6 * 60);
    h.tick(now, &mut o);
    assert_eq!(h.rooms(), 1, "gone after the grace period (one stays)");
}

#[test]
fn at_the_room_limit_an_abandoned_room_makes_space() {
    let (mut h, now) = hub_t(); // max 4
    let a = addr(1, 1);
    for i in 0..4u8 {
        let p = vec![1u8, b'A', ROOM_NEW, 1, b'0' + i];
        hsend(&mut h, now, a, &msg(ty::HELLO, 7, &p));
        hsend(&mut h, now, a, &msg(ty::LEAVE, 0, &[]));
    }
    assert_eq!(h.rooms(), 4);
    let b = addr(2, 1);
    let o = hsend(&mut h, now, b, &hello_room("B", ROOM_NEW));
    assert_eq!(welcome_slot(&o, b).0, 0, "a new room opens by closing the oldest empty one");
    assert_eq!(h.rooms(), 4);
}

#[test]
fn an_empty_room_can_be_deleted_but_an_occupied_or_last_one_cannot() {
    let (mut h, now) = hub_t();
    let (a, b, c) = (addr(1, 1), addr(2, 1), addr(3, 1));
    hsend(&mut h, now, a, &hello("A")); // room 1, occupied
    hsend(&mut h, now, b, &hello_room("B", ROOM_NEW)); // room 2
    hsend(&mut h, now, b, &msg(ty::LEAVE, 0, &[])); // room 2 is now empty
    assert_eq!(h.rooms(), 2);
    let o = hsend(&mut h, now, c, &msg(ty::DELROOM, 0, &[1])); // room 1 has A in it
    assert_eq!(roomlist(&o, c).len(), 2, "occupied: refused");
    let o = hsend(&mut h, now, c, &msg(ty::DELROOM, 0, &[2]));
    assert_eq!(roomlist(&o, c).len(), 1, "empty: deleted, and the new list comes back");
    hsend(&mut h, now, a, &msg(ty::LEAVE, 0, &[]));
    let o = hsend(&mut h, now, c, &msg(ty::DELROOM, 0, &[1]));
    assert_eq!(roomlist(&o, c).len(), 1, "the last room stays");
    let o = hsend(&mut h, now, c, &msg(ty::DELROOM, 0, &[99])); // unknown id: nothing happens
    assert_eq!(roomlist(&o, c).len(), 1);
}
