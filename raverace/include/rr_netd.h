/* rr_netd.h -- the built-in RRN1 lobby/relay server ("Host a game"): the same protocol as
 * server/ (Rust, NETPLAY.md), inside the game, so a LAN needs no separate server and no setup.
 * Polled from rr_net_poll once per frame; inert until started. */
#ifndef RR_NETD_H
#define RR_NETD_H
#include <stdbool.h>
bool rr_netd_start(int port);               /* false = the port is busy (another host / dev server) */
void rr_netd_stop(void);
bool rr_netd_running(void);
void rr_netd_set_name(const char *name);    /* the host's name, sent in discovery answers */
void rr_netd_poll(void);
#endif
