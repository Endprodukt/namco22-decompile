/* rr_env.c -- the System 22 board's half of the trace oracle's environment (engine/lift_env.c): how a keycus answer is forced and how
 * a 68K interrupt level is raised on this board's syscon lines. Dev trace builds only (RR_TRACE); RR_ENV=<file> from env_from_trace.py. */
#include <stdlib.h>
#include "lift_env.h"
#include "rr_hw.h"
#include "rr_mem.h"

static void irq_raise(int level)
{
    for (int line = 0; line < 5; line++)                 /* the syscon line that owns this level (rr_hw_irq_level's rule) */
        if ((g_rr.syscon[line] & 7) == level && (g_hw.irq_enabled & (1u << line))) {
            g_hw.irq_state |= 1u << line;
            if (line == 4) rr_hw_vblank();                   /* MAME's vblank interrupt is where handle_driving_io refreshes the inputs in shared RAM */
            break;
        }
}
static int in_env(uint32_t a)
{
    return (a >= 0x70000000u && a < 0x70020000u) || (a >= 0x60004000u && a < 0x60008000u) || (a >= 0x20000000u && a < 0x20000010u)
        || (a >= 0x20010000u && a < 0x20030000u) || (a >= 0x50000000u && a < 0x50000010u);   /* (the EEPROM's own reads are deterministic here: not reported) */
}
static void soft_prepare(uint32_t a)
{
    if (a >= 0x60004030u && a < 0x6000403Cu) rr_hw_drive_io();     /* the driving I/O words: our model knows what the board answers */
}
static const lift_env_board_t board = {
    .tag = "RR", .keycus_lo = 0x20000000u, .keycus_hi = 0x20000010u,
    .keycus_force = rr_hw_keycus_force, .in_env = in_env, .soft_prepare = soft_prepare, .irq_raise = irq_raise,
};
int rr_env_init(void) { return lift_env_init(getenv("RR_ENV"), &board); }
int rr_env_active(void) { return lift_env_active(); }
