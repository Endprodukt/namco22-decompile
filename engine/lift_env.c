/* lift_env.c -- see lift_env.h. Record format (tools/mame/env_from_trace.py):
 *   I <n> <level>                          an interrupt was taken just before main-flow instruction n
 *   E <flow> <n> <addr> <size> <value>     the read at instruction n of <flow> saw <value> at <addr>   (flow -1 = main, else the episode)
 *   B <flow> <n> <addr> <bit> <value>      the btst at instruction n saw <bit> of the byte at <addr> = <value>            */
#include <stdio.h>
#include <stdlib.h>
#include "lift_env.h"

#ifdef RR_TRACE
#include "lift_rt.h"
#include "lift_cpu.h"

typedef struct { char kind; int flow; uint64_t n; uint32_t addr; int size, bit, level; uint32_t value; int ckind, cc, taken; uint32_t imm, dn; } env_t;
static env_t *rec; static size_t nrec, pos;          /* main flow + interrupts, in trace order */
static env_t *erec; static size_t nerec, epos;        /* handler reads, by episode */
static uint64_t main_n, ep_n; static int ep_no = -1;
static uint32_t mism_addr[64]; static unsigned long mism_cnt[64]; static int n_mism; static unsigned long mism_total;
static int active;
static const lift_env_board_t *brd;

static void note_mismatch(uint32_t addr)
{
    mism_total++;
    for (int i = 0; i < n_mism; i++) if (mism_addr[i] == addr) { mism_cnt[i]++; return; }
    if (n_mism < 64) { mism_addr[n_mism] = addr; mism_cnt[n_mism++] = 1; }
}
static void report(void)
{
    if (!active) return;
    fprintf(stderr, "[%s] trace oracle: %lu environment reads differed from ours, at %d addresses:\n", brd->tag, mism_total, n_mism);
    for (int i = 0; i < n_mism; i++) fprintf(stderr, "       0x%08X x%lu\n", mism_addr[i], mism_cnt[i]);
}

/* every device read the replay had no record for: the instruction form env_from_trace.py does not understand (PC, address, count) */
static uint32_t applied[8]; static int n_applied;
static struct { uint32_t pc, addr; unsigned long n; } unrec[48]; static int n_unrec;
extern uint32_t rr_trace_pc;
static void probe(uint32_t a, int size)
{
    (void)size;
    if (!active || !brd->in_env || !brd->in_env(a)) return;
    for (int i = 0; i < n_applied; i++) if (applied[i] == a) return;
    for (int i = 0; i < n_unrec; i++) if (unrec[i].pc == rr_trace_pc && unrec[i].addr == a) { unrec[i].n++; return; }
    if (n_unrec < 48) { unrec[n_unrec].pc = rr_trace_pc; unrec[n_unrec].addr = a; unrec[n_unrec++].n = 1; }
}
extern void (*rr_read_probe)(uint32_t, int) __attribute__((weak));   /* System 22 only; Super 22 builds do not have the read probe */
static void report_unrec(void)
{
    for (int i = 0; i < n_unrec; i++) fprintf(stderr, "[%s] UNRECORDED device read at PC %06X of 0x%08X x%lu\n", brd->tag, unrec[i].pc, unrec[i].addr, unrec[i].n);
}

/* a compare/test of a device word: the branch that followed it in MAME's trace is the constraint; 68K flags of (dst - src), evaluated for our own value */
static int soft_ok(const env_t *e, uint32_t mem)
{
    const int bits = e->size * 8; const uint64_t m = bits == 32 ? 0xFFFFFFFFull : ((1ull << bits) - 1);
    uint64_t dst = e->ckind == 2 ? (e->dn & m) : (mem & m), src = e->ckind == 0 ? (e->imm & m) : e->ckind == 1 ? 0 : (mem & m);
    uint64_t r = (dst - src) & m;
    const int N = (r >> (bits - 1)) & 1, Z = r == 0, V = (int)((((dst ^ src) & (dst ^ r)) >> (bits - 1)) & 1), C = src > dst;
    int t;
    switch (e->cc) {                                    /* the order of CC in env_from_trace.py */
    case 0: case 1: t = !C; break;      case 2: case 3: t = C; break;
    case 4: t = Z; break;               case 5: t = !Z; break;
    case 6: t = !(C || Z); break;       case 7: t = C || Z; break;
    case 8: t = N == V; break;          case 9: t = N != V; break;
    case 10: t = !Z && N == V; break;   case 11: t = Z || N != V; break;
    case 12: t = N; break;              case 13: t = !N; break;
    case 14: t = !V; break;             default: t = V; break;
    }
    return t == e->taken;
}

static void apply(const env_t *e)
{
    if (n_applied < 8) applied[n_applied++] = e->addr;
    if (e->kind == 'S') {                               /* soft: override only if our own value takes the other branch */
        if (soft_ok(e, rr_read(e->addr, e->size))) return;
        if (brd->soft_prepare) { brd->soft_prepare(e->addr); if (soft_ok(e, rr_read(e->addr, e->size))) return; }
        rr_write(e->addr, e->size, e->value);
        return;
    }
    switch (e->kind) {
    case 'E':
        if (e->addr >= brd->keycus_lo && e->addr < brd->keycus_hi) { brd->keycus_force(e->value); break; }
        if (rr_read(e->addr, e->size) != e->value) note_mismatch(e->addr);
        rr_write(e->addr, e->size, e->value);
        break;
    case 'B': {
        uint32_t v = rr_read(e->addr, 1);
        if (((v >> e->bit) & 1u) != e->value) note_mismatch(e->addr);
        v = (v & ~(1u << e->bit)) | ((uint32_t)e->value << e->bit);
        rr_write(e->addr, 1, v);
        break; }
    case 'I':
        brd->irq_raise(e->level);
        rr_irq_enter(e->level);
        break;
    }
}

static void hook(uint32_t pc)
{
    (void)pc;
    n_applied = 0;
    if (rr_in_irq) {                                    /* a handler: its reads are keyed by episode */
        while (epos < nerec && (erec[epos].flow < ep_no || (erec[epos].flow == ep_no && erec[epos].n < ep_n))) epos++;
        while (epos < nerec && erec[epos].flow == ep_no && erec[epos].n == ep_n) apply(&erec[epos++]);
        ep_n++;
        return;
    }
    while (pos < nrec && rec[pos].n == main_n) {
        const env_t *e = &rec[pos++];
        if (e->kind == 'I') { ep_no++; ep_n = 0; }
        apply(e);                                       /* an 'I' runs its handler here, then the rest at n */
    }
    main_n++;
}

int lift_env_init(const char *path, const lift_env_board_t *board)
{
    if (!path || !board) return 0;
    FILE *f = fopen(path, "r");
    if (!f) { fprintf(stderr, "[%s] cannot open the environment file %s\n", board->tag, path); return 0; }
    brd = board;
    char line[128]; size_t cap = 0, ecap = 0;
    while (fgets(line, sizeof line, f)) {
        env_t e = {0}; unsigned long long n; unsigned a, v; int b, sz, lv, fl;
        if (line[0] == 'I' && sscanf(line + 2, "%llu %d", &n, &lv) == 2) { e.kind = 'I'; e.n = n; e.level = lv; }
        else if (line[0] == 'E' && sscanf(line + 2, "%d %llu %x %d %x", &fl, &n, &a, &sz, &v) == 5) { e.kind = 'E'; e.flow = fl; e.n = n; e.addr = a; e.size = sz; e.value = v; }
        else if (line[0] == 'S' && sscanf(line + 2, "%d %llu %x %d %x %d %u %u %d %d", &fl, &n, &a, &sz, &v, &e.ckind, &e.imm, &e.dn, &e.cc, &e.taken) == 10) { e.kind = 'S'; e.flow = fl; e.n = n; e.addr = a; e.size = sz; e.value = v; }
        else if (line[0] == 'B' && sscanf(line + 2, "%d %llu %x %d %d", &fl, &n, &a, &b, &sz) == 5) { e.kind = 'B'; e.flow = fl; e.n = n; e.addr = a; e.bit = b; e.value = (uint32_t)sz; }
        else continue;
        if (e.kind != 'I' && e.flow >= 0) {             /* a read inside an interrupt handler */
            if (nerec == ecap) { ecap = ecap ? ecap * 2 : 1024; erec = realloc(erec, ecap * sizeof *erec); }
            erec[nerec++] = e;
        } else {
            if (nrec == cap) { cap = cap ? cap * 2 : 1024; rec = realloc(rec, cap * sizeof *rec); }
            rec[nrec++] = e;
        }
    }
    fclose(f);
    fprintf(stderr, "[%s] trace oracle: %zu main-flow + %zu handler environment records from %s\n", board->tag, nrec, nerec, path);
    rr_trace_hook = hook;
    if (&rr_read_probe) rr_read_probe = probe;
    active = 1;
    atexit(report_unrec);
    atexit(report);                                     /* the trace build exits from inside the trace hook */
    return 1;
}
int lift_env_active(void) { return active; }
#else
int lift_env_init(const char *path, const lift_env_board_t *board) { (void)path; (void)board; return 0; }
int lift_env_active(void) { return 0; }
#endif
