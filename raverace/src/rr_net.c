/*
 * rr_net.c -- the RRN1 client (raverace/NETPLAY.md; the reference server is
 * server/, or the built-in host rr_netd.c).
 *
 * OFFLINE -> CONNECTING (HELLO, 500 ms retry) -> LOBBY (roster tracking,
 * READY/START, PING 1 Hz) -> SESSION (GO: the lobby slot becomes this
 * cabinet's link number, and the game's C139 link packets -- rr_link.c --
 * are exchanged as FRAMEs at 60 Hz). The game itself never knows the
 * difference from a wired 8-cabinet link: peer FRAMEs are injected through
 * rr_link_rx_push, one per frame at the frame edge (rr_link_poll), so with
 * N peers the injection cycles -- the game's own 8-frame peer timeout
 * tolerates that.
 *
 * Inert by default: no server configured -> OFFLINE -> rr_net_poll is one
 * branch. Debug counters with RR_NET_DEBUG=1; headless bootstrap with
 * RR_NET_SERVER / RR_NET_NAME / RR_NET_AUTOSTART (wired in rr_main.c).
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <SDL.h>                /* SDL_GetTicks (lazy-inits the timer: safe headless) */
#include "rr_sock.h"
#include "rr_netd.h"
#include "rr_net.h"
#include "rr_link.h"
#include "rr_hw.h"
#include "rr_ui.h"

#define RR_NET_PORT 27750
#define RETRY_MS    500          /* reliable client messages (HELLO/READY/START) */
#define PING_MS     1000         /* lobby keepalive, and the session fallback */
#define DEAD_MS     10000        /* server silence in connecting/lobby = lost */

enum { ST_OFFLINE, ST_CONNECTING, ST_LOBBY, ST_SESSION };
static const char *st_name[] = { "OFFLINE", "CONNECTING", "LOBBY", "SESSION" };

enum { T_HELLO = 0x01, T_WELCOME, T_ROSTER, T_READY, T_START, T_GO, T_LEAVE,
       T_FRAME, T_PING, T_PONG, T_ACK, T_DISCOVER, T_ANNOUNCE };

static int st = ST_OFFLINE;
static rr_sock_t sock = RR_SOCK_BAD;
static struct sockaddr_storage srv; static socklen_t srv_len;
static char srv_text[128];                       /* as given to rr_net_set_server */
static char my_name[24] = "PLAYER";
static char note[64];                            /* why we are offline, when we are */
static uint16_t next_seq = 1;
static int my_slot = -1;
static uint32_t session_id, frame_seq;
static uint32_t last_fseq[8];                    /* per-cabinet dedupe guard */
static struct { uint8_t slot, ready; char name[17]; } roster[8];
static int roster_n;
static uint32_t t_state, last_rx, last_ping_tx, last_frame_tx;
static uint32_t hello_tx, hello_seq;             /* CONNECTING's retransmit */
static int pend_ready, ready_val; static uint16_t ready_seq; static uint32_t ready_tx;
static int pend_start; static uint16_t start_seq; static uint32_t start_tx;
static int cab_reapply;                          /* frames left re-poking the cabinet number (the boot-time settings reload would overwrite the WRAM one) */
static int ping_ms = -1;
static int autostart = -1, debug = -1; static uint32_t autostart_t0;
static uint32_t n_ftx, n_frx, n_roster, n_poll;

static uint32_t now_ms(void) { return SDL_GetTicks(); }

/* ONLINE PLAY NEEDS NO COINS AND GIVES EVERYONE TIME: GO turns free play on (the cabinet's own setting comes back when the
 * session ends) and opens a 20 s window in which each player steps on the gas themselves -- what takes the game from attract
 * to its car select. A banner counts the window down on every machine. If a player has not pressed by then the machine
 * presses for them, so nobody is left behind. */
static int fp_saved = -1;                        /* the free-play setting before the session, or -1 */
static int wait_left, gas_left;                  /* frames left in the wait for the player's gas / of the automatic press */
#define GAS_WAIT_FRAMES (20 * 60)
#define AUTO_GAS_FRAMES 120
static void session_free_play(bool on)
{
    if (on) { if (fp_saved < 0) { fp_saved = rr_hw_freeplay() ? 1 : 0; rr_hw_set_freeplay(true); } }
    else if (fp_saved >= 0) { rr_hw_set_freeplay(fp_saved != 0); fp_saved = -1; }
}
void rr_net_apply_inputs(void)                   /* once per simulated frame, after the host's own input */
{
    if (st != ST_SESSION) { wait_left = gas_left = 0; return; }
    char msg[96];
    if (wait_left > 0) {
        if (g_hw.gas > 0x200) {                  /* the player stepped on the gas themselves */
            wait_left = 0;
            rr_ui_set_hint("Online race: go! Choose your car, then wait for the others at the course select", 240);
        } else {
            wait_left--;
            if (wait_left == 0) gas_left = AUTO_GAS_FRAMES;
            else if (wait_left % 30 == 0) {
                snprintf(msg, sizeof msg, "ONLINE RACE  -  step on the gas to start   (%d s)", (wait_left + 59) / 60);
                rr_ui_set_hint(msg, 45);
            }
        }
        return;
    }
    if (gas_left > 0) {
        g_hw.gas = 0x610; gas_left--;
        if (gas_left == AUTO_GAS_FRAMES - 1) rr_ui_set_hint("Online race: starting for you (no gas pressed)", 180);
    }
}

static void set_state(int ns)
{
    if (ns == st) return;
    if (st == ST_SESSION) session_free_play(false);
    if (debug == 1) fprintf(stderr, "[NET] %s -> %s\n", st_name[st], st_name[ns]);
    st = ns;
    t_state = now_ms();
    rr_link_net_active(st == ST_SESSION);        /* session: we own the link TX queue, loopback yields */
}

static void send_msg(int type, uint16_t seq, const void *pl, int len)
{
    uint8_t b[256];
    memcpy(b, "RRN1", 4);
    b[4] = (uint8_t)type; b[5] = 0;
    b[6] = (uint8_t)seq; b[7] = (uint8_t)(seq >> 8);
    b[8] = (uint8_t)len; b[9] = (uint8_t)(len >> 8);
    if (len) memcpy(b + 10, pl, (size_t)len);
    sendto(sock, (const char *)b, 10 + len, 0, (struct sockaddr *)&srv, srv_len);
}

static void send_hello(void)
{
    /* printable UTF-8, <= 16 bytes (the contract); retransmits keep the seq
     * (set in rr_net_connect), the WELCOME answers with it */
    uint8_t pl[17]; int n = 0;
    for (const char *c = my_name; *c && n < 16; c++)
        if (*c >= ' ' && (unsigned char)*c != 0x7F) pl[1 + n++] = (uint8_t)*c;
    if (!n) pl[1 + n++] = '?';
    pl[0] = (uint8_t)n;
    send_msg(T_HELLO, hello_seq, pl, 1 + n);
    hello_tx = now_ms();
}

/* datagrams count only from the server we talk to: the socket is unconnected,
 * so without this anyone who can reach the port could send GO/ROSTER/FRAME */
static int from_server(const struct sockaddr_storage *a) { return rr_addr_eq(a, &srv); }

static void sock_open(void)
{
    rr_sock_init();
    sock = socket(srv.ss_family, SOCK_DGRAM, 0);
    if (sock == RR_SOCK_BAD) return;
    rr_sock_nonblock(sock);
}

static void fail(const char *why)                /* lost/unreachable: close, OFFLINE with the reason */
{
    snprintf(note, sizeof note, "%s", why);
    if (sock != RR_SOCK_BAD) { closesocket(sock); sock = RR_SOCK_BAD; }
    pend_ready = pend_start = 0;
    my_slot = -1; roster_n = 0; session_id = 0;
    set_state(ST_OFFLINE);
}

bool rr_net_set_server(const char *host_port)
{
    rr_sock_init();
    if (st != ST_OFFLINE) rr_net_disconnect();
    char host[100], port[8] = "27750";
    const char *colon = strrchr(host_port, ':');
    if (colon && strchr(host_port, ':') == colon && colon != host_port) {  /* host:port (one colon: not IPv6) */
        size_t hl = (size_t)(colon - host_port);
        if (hl >= sizeof host) hl = sizeof host - 1;
        memcpy(host, host_port, hl); host[hl] = 0;
        snprintf(port, sizeof port, "%s", colon + 1);
    } else snprintf(host, sizeof host, "%s", host_port);
    struct addrinfo hint, *res = NULL;
    memset(&hint, 0, sizeof hint);
    hint.ai_family = AF_UNSPEC; hint.ai_socktype = SOCK_DGRAM;
    if (getaddrinfo(host, port, &hint, &res) != 0 || !res) {
        snprintf(note, sizeof note, "bad server address '%s'", host_port);
        if (debug == 1) fprintf(stderr, "[NET] %s\n", note);
        return false;
    }
    memcpy(&srv, res->ai_addr, res->ai_addrlen);
    srv_len = (socklen_t)res->ai_addrlen;
    freeaddrinfo(res);
    snprintf(srv_text, sizeof srv_text, "%s", host_port);
    note[0] = 0;
    return true;
}
const char *rr_net_server(void) { return srv_text; }

void rr_net_set_name(const char *name)
{
    snprintf(my_name, sizeof my_name, "%s", name && *name ? name : "PLAYER");
}
const char *rr_net_name(void) { return my_name; }

bool rr_net_connect(void)
{
    if (!srv_text[0]) { snprintf(note, sizeof note, "no server set"); return false; }
    if (st != ST_OFFLINE) return true;
    note[0] = 0;
    ping_ms = -1;
    sock_open();
    if (sock == RR_SOCK_BAD) { snprintf(note, sizeof note, "socket failed"); return false; }
    set_state(ST_CONNECTING);
    last_rx = now_ms();                          /* the connect attempt itself starts the silence clock */
    hello_seq = next_seq++;
    send_hello();
    return true;
}

void rr_net_disconnect(void)
{
    if (sock != RR_SOCK_BAD) {
        if (st == ST_LOBBY || st == ST_SESSION) send_msg(T_LEAVE, 0, NULL, 0);
        closesocket(sock); sock = RR_SOCK_BAD;
    }
    pend_ready = pend_start = 0;
    my_slot = -1; roster_n = 0; session_id = 0; ping_ms = -1;
    note[0] = 0;
    set_state(ST_OFFLINE);
}

void rr_net_status(char *buf, size_t n)
{
    switch (st) {
    case ST_OFFLINE:    snprintf(buf, n, note[0] ? "Offline (%s)" : "Offline", note); break;
    case ST_CONNECTING: snprintf(buf, n, "Connecting to %s ...", srv_text); break;
    case ST_LOBBY:      snprintf(buf, n, "%sLobby slot %d, %d player(s), ping %d ms%s%s", rr_netd_running() ? "Hosting: " : "", my_slot, roster_n, ping_ms, note[0] ? " - " : "", note); break;
    case ST_SESSION:    snprintf(buf, n, "Race armed (session %08X), slot %d", session_id, my_slot); break;
    }
}
bool rr_net_connected(void) { return st == ST_LOBBY || st == ST_SESSION; }
bool rr_net_session_active(void) { return st == ST_SESSION; }

bool rr_net_roster(int i, char *name, size_t n, int *ready, int *self)
{
    if (i < 0 || i >= roster_n) return false;
    snprintf(name, n, "%s", roster[i].name);
    *ready = roster[i].ready;
    *self = roster[i].slot == my_slot;
    return true;
}
int rr_net_roster_count(void) { return roster_n; }

void rr_net_set_ready(int ready)
{
    if (st != ST_LOBBY) return;
    if (debug == 1) fprintf(stderr, "[NET] ready -> %d\n", ready);
    ready_val = ready ? 1 : 0;
    pend_ready = 1;
    ready_seq = next_seq++;
    uint8_t b = (uint8_t)ready_val;
    send_msg(T_READY, ready_seq, &b, 1);
    ready_tx = now_ms();
}

static bool all_ready(void)
{
    if (roster_n < 2) return false;
    for (int i = 0; i < roster_n; i++) if (!roster[i].ready) return false;
    return true;
}
void rr_net_request_start(void)
{
    if (st != ST_LOBBY) return;
    if (!all_ready()) { snprintf(note, sizeof note, "waiting: everyone must be Ready"); if (debug == 1) fprintf(stderr, "[NET] start refused: not everyone is Ready (%d in roster)\n", roster_n); return; }
    note[0] = 0;
    pend_start = 1;
    start_seq = next_seq++;
    send_msg(T_START, start_seq, NULL, 0);
    start_tx = now_ms();
}

/* roster := u8 count; entries { u8 slot, u8 ready, u8 name_len, char name[] } */
static int parse_roster(const uint8_t *p, int len, int off)
{
    if (off >= len) return -1;
    int count = p[off++];
    if (count > 8) return -1;
    int n = 0;
    for (int i = 0; i < count; i++) {
        if (off + 3 > len) return -1;
        uint8_t slot = p[off], rdy = p[off + 1], nl = p[off + 2]; off += 3;
        if (off + nl > len) return -1;
        if (slot < 8) {
            int m = nl > 16 ? 16 : nl;           /* truncate long names (the contract) */
            roster[n].slot = slot; roster[n].ready = rdy ? 1 : 0;
            memcpy(roster[n].name, p + off, (size_t)m); roster[n].name[m] = 0;
            n++;
        }
        off += nl;
    }
    return n;
}

static uint32_t rd32(const uint8_t *p) { return p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24; }

static void roster_update(const uint8_t *p, int len, int off)
{
    int n = parse_roster(p, len, off);
    if (n < 0) return;
    roster_n = n;
    n_roster++;
    /* a ROSTER showing our new flag is READY's acknowledgment */
    for (int i = 0; i < n; i++)
        if (roster[i].slot == my_slot && pend_ready && roster[i].ready == ready_val) pend_ready = 0;
    if (debug == 1) {
        fprintf(stderr, "[NET] roster %d:", n);
        for (int i = 0; i < n; i++) fprintf(stderr, "  %d:%s%s", roster[i].slot, roster[i].name, roster[i].ready ? "*" : "");
        fprintf(stderr, "\n");
    }
}

static void on_go(uint16_t seq, const uint8_t *p, int len)
{
    if (st != ST_LOBBY && st != ST_SESSION) return;
    if (len < 5) return;
    uint32_t sid = rd32(p);
    uint8_t ack[2] = { (uint8_t)seq, (uint8_t)(seq >> 8) };
    send_msg(T_ACK, 0, ack, 2);                  /* the server retransmits GO until this */
    if (st == ST_SESSION && sid == session_id) return;   /* a retransmit: acked, done */
    int n = parse_roster(p, len, 4);
    if (n < 0) return;
    roster_n = n;
    int found = 0;
    for (int i = 0; i < n; i++) if (roster[i].slot == my_slot) found = 1;
    if (!found) { fail("not in the GO roster"); return; }
    session_id = sid;
    frame_seq = 0;
    for (int i = 0; i < 8; i++) last_fseq[i] = 0xFFFFFFFFu;
    pend_start = pend_ready = 0;
    /* the lobby slot IS the cabinet number; the EEPROM patch wins only after the
     * boot-time settings reload (~frame 300), so keep re-poking for a while */
    rr_hw_set_link_cabinet(my_slot);
    cab_reapply = 360;
    set_state(ST_SESSION);
    session_free_play(true);
    wait_left = GAS_WAIT_FRAMES; gas_left = 0;
    fprintf(stderr, "[NET] GO: session %08X, slot %d, %d players\n", sid, my_slot, n);
}

static void on_frame(const uint8_t *p, int len)
{
    if (st != ST_SESSION || len != 49) return;
    if (rd32(p) != session_id) return;                     /* another session's traffic */
    uint32_t fs = rd32(p + 4);
    uint8_t cab = p[8];
    if (cab >= 8 || cab == my_slot) return;
    if (last_fseq[cab] != 0xFFFFFFFFu && fs <= last_fseq[cab]) return;   /* dedupe / reorder guard */
    last_fseq[cab] = fs;
    /* the 40 wire bytes are the 38-byte game payload + 2 zero fill bytes */
    rr_link_pkt_t pkt;
    memcpy(pkt.data, p + 9, RR_LINK_PKT_LEN);
    pkt.id = cab;
    pkt.data[0] = 0; pkt.data[1] = cab;                    /* the receive path's sender check */
    rr_link_rx_push(&pkt);
    n_frx++;
}

static void on_message(int type, uint16_t seq, const uint8_t *p, int len)
{
    switch (type) {
    case T_WELCOME:
        if (st != ST_CONNECTING || seq != hello_seq || len < 6) return;
        if (p[0] == 0xFF) { fail(p[1] == 1 ? "race in progress, try later" : "lobby full"); return; }
        if (p[0] >= 8) return;
        my_slot = p[0];
        roster_update(p, len, 6);
        fprintf(stderr, "[NET] joined %s as slot %d ('%s')\n", srv_text, my_slot, my_name);
        set_state(ST_LOBBY);
        break;
    case T_ROSTER:
        if (len < 5) return;
        if (st == ST_SESSION && p[0] == 0) {               /* the server ended the session */
            session_id = 0;
            set_state(ST_LOBBY);
        }
        if (st == ST_LOBBY || st == ST_SESSION) roster_update(p, len, 5);
        break;
    case T_GO: on_go(seq, p, len); break;
    case T_FRAME: on_frame(p, len); break;
    case T_PONG:
        if (len >= 4) ping_ms = (int)(now_ms() - rd32(p));
        break;
    default: break;                                        /* unknown types: ignore (the contract) */
    }
}

static void send_frame(void)
{
    rr_link_pkt_t p;
    if (!rr_link_tx_pop(&p)) return;
    uint8_t pl[49];
    pl[0] = (uint8_t)session_id; pl[1] = (uint8_t)(session_id >> 8);
    pl[2] = (uint8_t)(session_id >> 16); pl[3] = (uint8_t)(session_id >> 24);
    pl[4] = (uint8_t)frame_seq; pl[5] = (uint8_t)(frame_seq >> 8);
    pl[6] = (uint8_t)(frame_seq >> 16); pl[7] = (uint8_t)(frame_seq >> 24);
    if (debug == 1 && p.id != my_slot && n_ftx % 60 == 0) fprintf(stderr, "[NET] staged id %d != slot %d\n", p.id, my_slot);
    pl[8] = p.id;                                            /* our cabinet number = our slot */
    memcpy(pl + 9, p.data, RR_LINK_PKT_LEN);
    pl[9 + RR_LINK_PKT_LEN] = pl[10 + RR_LINK_PKT_LEN] = 0;  /* 38 + 2 zero fill = 40 */
    frame_seq++;
    send_msg(T_FRAME, 0, pl, 49);
    last_frame_tx = now_ms();
    n_ftx++;
}

static void send_ping(uint32_t now)
{
    uint8_t pl[4] = { (uint8_t)now, (uint8_t)(now >> 8), (uint8_t)(now >> 16), (uint8_t)(now >> 24) };
    send_msg(T_PING, 0, pl, 4);
    last_ping_tx = now;
}


/* ---- LAN discovery --------------------------------------------------------
 * DISCOVER is broadcast on the LAN (limited broadcast, every interface's directed
 * broadcast where the OS lists them, and loopback for a host on this machine);
 * every host answers ANNOUNCE { state, players, name }. Results are kept until the
 * next search. Runs for 4 s (resent each second), on its own socket. */
#define DISC_MS 4000
static rr_sock_t dsock = RR_SOCK_BAD;
static uint32_t d_start, d_tx; static int d_auto;
static struct { struct sockaddr_storage a; char addr[64], name[17]; uint8_t state, players; } found[8];
static int found_n;

static void disc_send_one(uint32_t ip_be)
{
    uint8_t b[10]; memcpy(b, "RRN1", 4); b[4] = T_DISCOVER; b[5] = 0; b[6] = b[7] = b[8] = b[9] = 0;
    struct sockaddr_in a; memset(&a, 0, sizeof a);
    a.sin_family = AF_INET; a.sin_addr.s_addr = ip_be; a.sin_port = htons(RR_NET_PORT);
    sendto(dsock, (const char *)b, sizeof b, 0, (struct sockaddr *)&a, sizeof a);
}
static void disc_send(void)
{
    disc_send_one(htonl(INADDR_BROADCAST));
    disc_send_one(htonl(INADDR_LOOPBACK));                       /* a host on this machine */
#ifndef _WIN32
    struct ifaddrs *ifs = NULL;
    if (getifaddrs(&ifs) == 0) {
        for (struct ifaddrs *i = ifs; i; i = i->ifa_next)
            if (i->ifa_addr && i->ifa_broadaddr && i->ifa_addr->sa_family == AF_INET && (i->ifa_flags & IFF_BROADCAST) && (i->ifa_flags & IFF_UP))
                disc_send_one(((struct sockaddr_in *)i->ifa_broadaddr)->sin_addr.s_addr);
        freeifaddrs(ifs);
    }
#endif
    d_tx = now_ms();
}

void rr_net_discover(void)
{
    rr_sock_init();
    if (dsock != RR_SOCK_BAD) { closesocket(dsock); dsock = RR_SOCK_BAD; }
    dsock = socket(AF_INET, SOCK_DGRAM, 0);
    if (dsock == RR_SOCK_BAD) { snprintf(note, sizeof note, "socket failed"); return; }
    int on = 1; setsockopt(dsock, SOL_SOCKET, SO_BROADCAST, (const char *)&on, sizeof on);
    rr_sock_nonblock(dsock);
    found_n = 0;
    d_start = now_ms();
    disc_send();
}
bool rr_net_discovering(void) { return dsock != RR_SOCK_BAD; }
void rr_net_discover_autojoin(int on) { d_auto = on; }
int rr_net_found_count(void) { return found_n; }
bool rr_net_found(int i, char *label, size_t n, char *addr, size_t an)
{
    if (i < 0 || i >= found_n) return false;
    snprintf(label, n, "%s  %s  %d player(s)%s", found[i].name, found[i].addr, found[i].players, found[i].state ? "  [race running]" : "");
    if (addr) snprintf(addr, an, "%s", found[i].addr);
    return true;
}

static void disc_poll(void)
{
    if (dsock == RR_SOCK_BAD) return;
    uint8_t b[512];
    for (int k = 0; k < 16; k++) {
        struct sockaddr_storage from; socklen_t fl = sizeof from;
        int n = (int)recvfrom(dsock, (char *)b, sizeof b, 0, (struct sockaddr *)&from, &fl);
        if (n < 0) break;
        if (n < 13 || memcmp(b, "RRN1", 4) || b[4] != T_ANNOUNCE) continue;
        if ((b[8] | b[9] << 8) != n - 10) continue;
        const uint8_t *p = b + 10; int nl = p[2]; if (n - 13 < nl) continue;
        int idx = -1;
        for (int i = 0; i < found_n; i++) if (rr_addr_eq(&found[i].a, &from)) idx = i;
        if (idx < 0) { if (found_n >= 8) continue; idx = found_n++; }
        found[idx].a = from;
        char host[64] = "?"; getnameinfo((struct sockaddr *)&from, fl, host, sizeof host, NULL, 0, NI_NUMERICHOST);
        uint16_t port = from.ss_family == AF_INET ? ntohs(((struct sockaddr_in *)&from)->sin_port) : RR_NET_PORT;
        snprintf(found[idx].addr, sizeof found[idx].addr, "%s:%u", host, port);
        found[idx].state = p[0]; found[idx].players = p[1];
        if (nl > 16) nl = 16;
        int m = 0; for (int i = 0; i < nl; i++) if (p[3 + i] >= ' ' && p[3 + i] != 0x7F) found[idx].name[m++] = (char)p[3 + i];
        found[idx].name[m] = 0;
        if (debug == 1) fprintf(stderr, "[NET] found %s '%s' %d player(s)\n", found[idx].addr, found[idx].name, found[idx].players);
    }
    uint32_t now = now_ms();
    if (now - d_tx >= 1000 && now - d_start < DISC_MS) disc_send();
    if (now - d_start >= DISC_MS) { closesocket(dsock); dsock = RR_SOCK_BAD; }
    if (d_auto && found_n > 0 && st == ST_OFFLINE && !found[0].state) {     /* headless test: join the first host found */
        d_auto = 0;
        if (rr_net_set_server(found[0].addr)) rr_net_connect();
    }
}

/* ---- hosting: the built-in server, and this client joined to it over loopback ---- */
bool rr_net_host_start(void)
{
    if (rr_netd_running()) return true;
    if (st != ST_OFFLINE) rr_net_disconnect();
    if (!rr_netd_start(RR_NET_PORT)) { snprintf(note, sizeof note, "cannot host: port %d is busy", RR_NET_PORT); return false; }
    rr_netd_set_name(my_name);
    if (!rr_net_set_server("127.0.0.1")) { rr_netd_stop(); return false; }
    return rr_net_connect();
}
void rr_net_host_stop(void) { rr_net_disconnect(); rr_netd_stop(); }
bool rr_net_hosting(void) { return rr_netd_running(); }

void rr_net_poll(void)
{
    if (debug < 0) { const char *e = getenv("RR_NET_DEBUG"); debug = e && *e == '1'; }
    if (autostart < 0) { const char *e = getenv("RR_NET_AUTOSTART"); autostart = e && *e == '1'; }
    rr_netd_poll();                                        /* the built-in host (inert unless hosting) */
    disc_poll();                                           /* LAN search (inert unless searching) */
    if (st == ST_OFFLINE) return;                          /* inert: no client socket, no syscalls */
    uint32_t now = now_ms();

    uint8_t b[512];
    for (int i = 0; i < 32; i++) {                         /* bounded: never stall the frame */
        struct sockaddr_storage from; socklen_t fl = sizeof from;
        int n = (int)recvfrom(sock, (char *)b, sizeof b, 0, (struct sockaddr *)&from, &fl);
        if (n < 0) {
            if (!rr_sock_would_block()) { if (st != ST_SESSION) fail("socket error"); }
            break;
        }
        if (!from_server(&from)) continue;                 /* not the server: drop */
        if (n < 10 || memcmp(b, "RRN1", 4)) continue;
        int len = b[8] | b[9] << 8;
        if (len != n - 10) continue;                       /* bad length: drop (the contract) */
        last_rx = now;
        on_message(b[4], (uint16_t)(b[6] | b[7] << 8), b + 10, len);
    }

    switch (st) {
    case ST_CONNECTING:
        if (now - hello_tx >= RETRY_MS) send_hello();      /* the WELCOME's seq matches hello_seq */
        if (now - last_rx > DEAD_MS) fail("no response from server");
        break;
    case ST_LOBBY:
        if (pend_ready && now - ready_tx >= RETRY_MS) { uint8_t v = (uint8_t)ready_val; send_msg(T_READY, ready_seq, &v, 1); ready_tx = now; }
        if (pend_start && now - start_tx >= RETRY_MS) { send_msg(T_START, start_seq, NULL, 0); start_tx = now; }
        if (now - last_ping_tx >= PING_MS) send_ping(now);
        if (now - last_rx > DEAD_MS) { fail("connection lost"); break; }
        if (autostart) {                                   /* headless test: start once the lobby has settled at 2+ */
            if (roster_n >= 2) {
                if (!autostart_t0) autostart_t0 = now;
                else if (now - autostart_t0 >= 2000 && !pend_start) { for (int i = 0; i < roster_n; i++) if (roster[i].slot == my_slot && !roster[i].ready && !pend_ready) rr_net_set_ready(1); if (all_ready()) rr_net_request_start(); }
            } else autostart_t0 = 0;
        }
        break;
    case ST_SESSION:
        if (cab_reapply > 0) { rr_hw_set_link_cabinet(my_slot); cab_reapply--; }
        send_frame();                                      /* one staged link packet per frame */
        if (now - last_frame_tx >= PING_MS && now - last_ping_tx >= PING_MS)
            send_ping(now);                                /* no car to report: PING is the liveness fallback */
        /* a dead server mid-race is NOT fatal: the game's own 8-frame peer
         * timeout drops the rivals and the race plays out alone */
        break;
    }

    if (debug == 1 && ++n_poll % 60 == 0)
        fprintf(stderr, "[NET] %s slot %d  frames tx %u rx %u  roster %u  ping %d ms\n",
                st_name[st], my_slot, n_ftx, n_frx, n_roster, ping_ms);
}
