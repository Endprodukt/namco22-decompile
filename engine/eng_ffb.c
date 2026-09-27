/* eng_ffb.c -- see eng_ffb.h */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "eng_ffb.h"

static SDL_Haptic *haptic;
static int effect = -1;
static SDL_JoystickID dev = -1;                 /* the device last tried, so a wheel without FFB is not retried every frame */
static int applied;

int eng_ffb_decode(uint8_t b)
{
    int r = 0;                                  /* bits 2-7, bit order reversed */
    for (int i = 0; i < 6; i++) if (b >> (2 + i) & 1) r |= 1 << (5 - i);
    if (r == 0) return 0;                       /* 63: never sent (Dirt Dash stops at 62, Rave Racer at 60) -- RAM not written yet */
    return (b & 2) ? r - 63 : 63 - r;
}

/* SDL's direction is where a force comes FROM: along +x a positive level pushes toward the axis' negative end */
static SDL_HapticEffect constant(int level)
{
    SDL_HapticEffect e;
    memset(&e, 0, sizeof e);
    e.type = SDL_HAPTIC_CONSTANT;
    e.constant.direction.type = SDL_HAPTIC_CARTESIAN;
    e.constant.direction.dir[0] = 1;
    e.constant.length = SDL_HAPTIC_INFINITY;
    e.constant.level = (Sint16)level;
    return e;
}

static void release(void)
{
    if (haptic && effect >= 0) {                /* a wheel left holding the last force would keep pushing after we are gone */
        SDL_HapticStopEffect(haptic, effect);
        SDL_HapticDestroyEffect(haptic, effect);
    }
    if (haptic) SDL_HapticClose(haptic);
    haptic = NULL; effect = -1;
}

void eng_ffb_close(void) { release(); dev = -1; }

void eng_ffb_forget(SDL_JoystickID id) { if (id == dev) eng_ffb_close(); }

bool eng_ffb_device(SDL_Joystick *js)
{
    const SDL_JoystickID id = js ? SDL_JoystickInstanceID(js) : -1;
    if (id == dev) return effect >= 0;
    release();
    dev = id;
    if (!js) return false;
    if (!SDL_WasInit(SDL_INIT_HAPTIC) && SDL_InitSubSystem(SDL_INIT_HAPTIC) != 0) {
        fprintf(stderr, "[FFB] SDL haptic: %s\n", SDL_GetError());
        return false;
    }
    const char *name = SDL_JoystickName(js) ? SDL_JoystickName(js) : "joystick";
    if (!SDL_JoystickIsHaptic(js) || !(haptic = SDL_HapticOpenFromJoystick(js))) {
        fprintf(stderr, "[FFB] %s: no force feedback\n", name);
        return false;
    }
    const unsigned q = SDL_HapticQuery(haptic);
    if (!(q & SDL_HAPTIC_CONSTANT)) {
        fprintf(stderr, "[FFB] %s: no constant force\n", name);
        release();
        return false;
    }
    if (q & SDL_HAPTIC_AUTOCENTER) SDL_HapticSetAutocenter(haptic, 0);        /* the motor is the only force, as in the cabinet */
    if (q & SDL_HAPTIC_GAIN) SDL_HapticSetGain(haptic, 100);
    SDL_HapticEffect e = constant(0);
    effect = SDL_HapticNewEffect(haptic, &e);
    if (effect < 0 || SDL_HapticRunEffect(haptic, effect, 1) != 0) {
        fprintf(stderr, "[FFB] %s: %s\n", name, SDL_GetError());
        release();
        return false;
    }
    applied = 0;
    static bool at_exit;
    if (!at_exit) { atexit(eng_ffb_close); at_exit = true; }      /* every way out that ends in exit() */
    fprintf(stderr, "[FFB] %s drives the wheel motor\n", name);
    return true;
}

void eng_ffb_force(int motor, int strength, bool reverse)
{
    if (effect < 0) return;
    /* a negative command pushes toward the higher A-D side: the axis' positive end unless reversed */
    int level = motor * 32767 / 63 * strength / 100;
    if (reverse) level = -level;
    if (level == applied) return;
    SDL_HapticEffect e = constant(level);
    if (SDL_HapticUpdateEffect(haptic, effect, &e) == 0) applied = level;
}
