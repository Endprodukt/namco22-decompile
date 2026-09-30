/* lift_env.h -- the TRACE ORACLE's environment for a lifted 68K, board-independent (dev builds only: RR_TRACE).
 * What the 68K sees that is not the 68K -- a device's answer, and the instruction an interrupt lands on -- is taken from MAME's own
 * register trace (a game's tools/mame/env_from_trace.py) and replayed, so the LIFTED program can be compared with MAME instruction
 * for instruction before the DSP and MCU exist as code. The board supplies two hooks: how a keycus answer is forced, and how an
 * interrupt of a given 68K level is raised. (engine/ss22_env.c is the Super 22 copy of this and should migrate onto it.) */
#ifndef LIFT_ENV_H
#define LIFT_ENV_H
#include <stdint.h>

typedef struct {
    const char *tag;                                   /* for the log */
    uint32_t keycus_lo, keycus_hi;                     /* [lo, hi): a read here is a random answer to force, not to write */
    void (*keycus_force)(uint32_t value);
    int  (*in_env)(uint32_t addr);                     /* the board's device addresses: a read there with no record is reported (an unrecognised instruction form) */
    void (*soft_prepare)(uint32_t addr);               /* before a compare-derived (soft) record overrides addr: let the board's own model produce its value first */
    void (*irq_raise)(int level);                      /* mark the board's IRQ line for this level pending (the handler is entered after) */
} lift_env_board_t;

int  lift_env_init(const char *path, const lift_env_board_t *board);   /* 0 = not built in / cannot open */
int  lift_env_active(void);
#endif
