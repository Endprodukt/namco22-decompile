/* rr_sock.h -- the small UDP portability layer shared by rr_net.c (client) and rr_netd.c (built-in host). */
#ifndef RR_SOCK_H
#define RR_SOCK_H
#include <stdint.h>
#include <string.h>
#ifdef _WIN32
/* winsock2/ws2tcpip come in through the force-included win_compat.h */
typedef SOCKET rr_sock_t;
#define RR_SOCK_BAD INVALID_SOCKET
static inline int rr_sock_would_block(void) { return WSAGetLastError() == WSAEWOULDBLOCK || WSAGetLastError() == WSAECONNRESET; }
#else
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <ifaddrs.h>
#include <net/if.h>
typedef int rr_sock_t;
#define RR_SOCK_BAD (-1)
#define closesocket close
static inline int rr_sock_would_block(void) { return errno == EAGAIN || errno == EWOULDBLOCK || errno == ECONNREFUSED; }
#endif

static inline void rr_sock_init(void)              /* Winsock must be up before getaddrinfo AND socket */
{
#ifdef _WIN32
    static int wsa;
    if (!wsa) { WSADATA w; wsa = WSAStartup(MAKEWORD(2, 2), &w) == 0; }
#endif
}
static inline void rr_sock_nonblock(rr_sock_t s)
{
#ifdef _WIN32
    u_long nb = 1; ioctlsocket(s, FIONBIO, &nb);
#else
    fcntl(s, F_SETFL, fcntl(s, F_GETFL) | O_NONBLOCK);
#endif
}
static inline int rr_addr_eq(const struct sockaddr_storage *a, const struct sockaddr_storage *b)
{
    if (a->ss_family != b->ss_family) return 0;
    if (a->ss_family == AF_INET) {
        const struct sockaddr_in *x = (const struct sockaddr_in *)a, *y = (const struct sockaddr_in *)b;
        return x->sin_port == y->sin_port && x->sin_addr.s_addr == y->sin_addr.s_addr;
    }
    if (a->ss_family == AF_INET6) {
        const struct sockaddr_in6 *x = (const struct sockaddr_in6 *)a, *y = (const struct sockaddr_in6 *)b;
        return x->sin6_port == y->sin6_port && !memcmp(&x->sin6_addr, &y->sin6_addr, sizeof x->sin6_addr);
    }
    return 0;
}
#endif
