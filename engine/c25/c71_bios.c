/* c71_bios.c -- the C71 DSP's BIOS, built in.
 *
 * 4096 16-bit words at program memory 0: the DSP's boot code and its tables, the same file in every System 22 / Super 22 game (MAME's namcoc71 set, c71.bin).
 * It used to be read from the ROM folder, which made every game need c71.bin -- inside the game's own zip in some MAME sets, in a separate namcoc71.zip in
 * others. The words come from engine/c25/c71_bios.inc, made by tools/gen/c71_embed.py (which refuses anything but the known-good dump); the build-time
 * translator (tools/gen/c25_translate.py) reads the same file, so the run-time words and the translated code cannot disagree. */
#include <string.h>
#include "c25.h"

static const uint16_t c71_bios_words[0x1000] = {
#include "c71_bios.inc"
};

void c71_load_builtin_bios(c71_t *d)
{
    memcpy(d->prog, c71_bios_words, sizeof c71_bios_words);
}
