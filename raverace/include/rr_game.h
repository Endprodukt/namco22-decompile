/* rr_game.h -- what differs between the System 22 games this runtime hosts (HARD RULE 3: one runtime, tweaks per game).
 * Rave Racer's table is the default (src/rr_game.c); another game (acedriver/) supplies its own table instead of that file. */
#ifndef RR_GAME_H
#define RR_GAME_H
#include <stddef.h>
#include <stdint.h>

typedef struct { const char *file; uint32_t at; } rr_wave_t;

typedef struct {
    const char *name;                       /* "raverace", "acedrive" ... */
    int autosetup;                          /* 1: src/rr_romzip.c unpacks this game's zips on first run (its chip table is Rave Racer's); 0: the ROM files must be in extracted/ */
    void (*entry)(void);                    /* the lifted reset routine (the 68020 program's reset PC): never returns */
    const char *prg[4];                     /* the four 68020 program chips: MAME ROM_LOAD32_BYTE offsets 0..3 (uu, um, lm, ll) */
    const char *cg[8];                      /* texture chips, 2 MB each at slot i (NULL = the slot is empty: Ace Driver has slots 4..7 only) */
    const char *ccrl, *ccrh;                /* texture tilemap + attributes */
    const char *pot[3][4];                  /* point ROM: low / mid / upper byte planes, pot_chips chips of 512 KB each (NULL past that) */
    int pot_chips;                          /* 4 (Rave Racer), 2 (Ace Driver), 3 (Victory Lap) */
    const char *snd_data;                   /* the sound MCU's program + data ROM */
    rr_wave_t wav[4];                       /* C352 wave ROMs and where each sits */
    const char *gamma[3];                   /* gamma PROMs */
    const char *eeprom;                     /* default EEPROM image, or NULL (blank: the game initialises it) */
    int keycus_off; uint16_t keycus_val;    /* MAME namcos22_keycus_r: this word offset returns keycus_val, every other read is random; -1 = all random */
    int steer_add, gas_add, brake_add;      /* MAME handle_driving_io: the offsets added to the axes in shared RAM 0x32..0x36 */
    uint16_t spin_pc, spin_op;              /* the master DSP program's busy-wait poll (0 = none: nothing is fast-forwarded) */
} rr_game_t;

extern const rr_game_t *g_rr_game;
#endif
