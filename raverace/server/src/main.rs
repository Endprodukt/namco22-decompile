//! rrn1-server [--port 27750] [--bind 0.0.0.0] [--name NAME] [--max-per-ip N] [--max-rooms N] [--room-ttl-min N] [--max-session-min N] [--frame-idle-sec N] [--quiet]
//!
//! Lobby + relay for Rave Racer online play. Single thread, std only: a UDP socket with a short
//! read timeout drives `Hub::handle` / `Hub::tick` (one `Server` per room).

use rrn1_server::{Config, Hub, Out, DEFAULT_PORT};
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn usage() -> ! {
    eprintln!("usage: rrn1-server [--port N] [--bind ADDR] [--name NAME] [--max-per-ip N] [--max-rooms N] [--max-session-min N] [--frame-idle-sec N] [--quiet]");
    std::process::exit(2);
}

fn main() {
    let mut port = DEFAULT_PORT;
    let mut bind = String::from("0.0.0.0");
    let mut cfg = Config::default();
    let mut quiet = false;
    let mut max_rooms = 16usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = |what: &str| args.next().unwrap_or_else(|| { eprintln!("{} needs a value", what); usage() });
        match a.as_str() {
            "--port" => port = val("--port").parse().unwrap_or_else(|_| usage()),
            "--bind" => bind = val("--bind"),
            "--name" => cfg.name = val("--name"),
            "--room-ttl-min" => cfg.room_ttl = Duration::from_secs(60 * val("--room-ttl-min").parse::<u64>().unwrap_or_else(|_| usage())),
            "--max-rooms" => max_rooms = val("--max-rooms").parse().unwrap_or_else(|_| usage()),
            "--max-per-ip" => cfg.max_per_ip = val("--max-per-ip").parse().unwrap_or_else(|_| usage()),
            "--max-session-min" => cfg.max_session = Duration::from_secs(60 * val("--max-session-min").parse::<u64>().unwrap_or_else(|_| usage())),
            "--frame-idle-sec" => cfg.frame_idle = Duration::from_secs(val("--frame-idle-sec").parse().unwrap_or_else(|_| usage())),
            "--quiet" => quiet = true,
            _ => usage(),
        }
    }

    let sock = UdpSocket::bind((bind.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("[rrn1] cannot bind {}:{}: {}", bind, port, e);
        std::process::exit(1);
    });
    sock.set_read_timeout(Some(Duration::from_millis(50))).ok();
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
        ^ ((std::process::id() as u64) << 32);
    let mut srv = Hub::new(cfg, max_rooms, Instant::now(), seed);
    if !quiet {
        println!("[rrn1] server on UDP {}:{}", bind, port);
    }

    let mut buf = [0u8; 1500];
    let mut out: Out = Vec::new();
    loop {
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => srv.handle(&buf[..n], from, Instant::now(), &mut out),
            // a timeout is the tick; ConnectionReset is Windows reporting an ICMP unreachable for a past send
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset) => {}
            Err(e) => {
                eprintln!("[rrn1] recv: {}", e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        srv.tick(Instant::now(), &mut out);
        for (to, d) in out.drain(..) {
            let _ = sock.send_to(&d, to);
        }
        for e in srv.events.drain(..) {
            if !quiet {
                println!("[rrn1] {}", e);
            }
        }
    }
}
