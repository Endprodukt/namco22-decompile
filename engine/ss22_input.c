/* ss22_input.c -- see ss22_input.h. Shared keyboard, pad and raw wheel/joystick input for Super System 22 games. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "ss22_input.h"
#include "eng_cfg.h"
#include "eng_ui.h"

#define PAD_DEADZONE   8000
#define WHEEL_DEADZONE 1000
#define MAX_ACTIONS    16
#define MAX_DEV        8
#define MAX_CAP_AXES   16

static const ss22_input_game *game;
static SDL_Scancode bound[MAX_ACTIONS];
static int rebinding = -1;                       /* action waiting for a keyboard key, or -1 */
static bool test_latch, test_prev[MAX_ACTIONS];
static int service_frames;

/* ---- raw wheels / joysticks -------------------------------------------------
 * SDL game controllers keep their standard layout. Everything else (wheels,
 * pedal sets, arcade controls) is opened as SDL_Joystick.
 *
 * Axis settings use the same syntax as Rave Racer:
 *   joy_steer = <axis>[ invert]
 *   joy_gas   = <axis>[ invert][ positive|negative]
 *   joy_brake = <axis>[ invert][ positive|negative]
 *
 * A Controls-menu bind records the SDL GUID too, so a separate USB pedal set
 * is not confused with the wheel base. "positive"/"negative" is learnt from
 * the direction in which the pedal moved while binding.
 */
typedef struct {
    int axis;
    bool invert;
    int direction;                              /* 0 auto, +1 pressed raises value, -1 lowers it */
    char guid[40];                              /* SDL GUID string; empty = first raw device */
} raw_axis_bind;

typedef struct {
    SDL_Joystick *js;
    SDL_JoystickID id;
    char guid[40];
    char name[96];
} raw_dev;

typedef struct {
    bool rest_valid;
    int rest;
    int learned_direction;
} pedal_cal;

static raw_axis_bind joy_steer = { 0, false, 0, "" };
static raw_axis_bind joy_gas   = { -1, false, 0, "" };
static raw_axis_bind joy_brake = { -1, false, 0, "" };
static int joy_button[MAX_ACTIONS];
static raw_dev raws[MAX_DEV];
static pedal_cal gas_cal, brake_cal;

/* axis capture in the Controls menu: 0 steer, 1 gas, 2 brake */
static int axis_capture = -1;
static int cap_base[MAX_DEV][MAX_CAP_AXES];
static bool cap_valid[MAX_DEV][MAX_CAP_AXES];

static const char *axis_cfg_name(int k) { return k == 0 ? "joy_steer" : k == 1 ? "joy_gas" : "joy_brake"; }
static const char *guid_cfg_name(int k) { return k == 0 ? "joy_steer_guid" : k == 1 ? "joy_gas_guid" : "joy_brake_guid"; }
static raw_axis_bind *axis_bind(int k) { return k == 0 ? &joy_steer : k == 1 ? &joy_gas : &joy_brake; }

static void parse_axis(raw_axis_bind *b, const char *key, int def_axis)
{
    const char *v = eng_cfg_get(key);
    b->axis = v && *v ? atoi(v) : def_axis;
    b->invert = v && strstr(v, "invert") != NULL;
    b->direction = v && strstr(v, "positive") ? +1 : v && strstr(v, "negative") ? -1 : 0;
}

static void raw_bindings_load(void)
{
    parse_axis(&joy_steer, "joy_steer", 0);
    parse_axis(&joy_gas, "joy_gas", -1);
    parse_axis(&joy_brake, "joy_brake", -1);
    for (int k = 0; k < 3; k++) {
        raw_axis_bind *b = axis_bind(k);
        const char *g = eng_cfg_get(guid_cfg_name(k));
        snprintf(b->guid, sizeof b->guid, "%s", g ? g : "");
    }
    for (int a = 0; a < MAX_ACTIONS; a++) joy_button[a] = -1;
    for (int a = 0; a < game->n && a < MAX_ACTIONS; a++) {
        char k[64]; snprintf(k, sizeof k, "joy_%s", game->actions[a].key);
        joy_button[a] = eng_cfg_int(k, -1);
    }
}

static void raw_bind_save(int kind)
{
    raw_axis_bind *b = axis_bind(kind);
    char v[64];
    snprintf(v, sizeof v, "%d%s%s", b->axis, b->invert ? " invert" : "",
             b->direction > 0 ? " positive" : b->direction < 0 ? " negative" : "");
    eng_cfg_set(axis_cfg_name(kind), v);
    eng_cfg_set(guid_cfg_name(kind), b->guid);
}

/* ---- keyboard bindings --------------------------------------------------- */
static void bindings_load(void)
{
    for (int a = 0; a < game->n; a++) {
        bound[a] = game->actions[a].def;
        char k[48]; snprintf(k, sizeof k, "key_%s", game->actions[a].key);
        const char *v = eng_cfg_get(k);
        if (v && *v) {
            const SDL_Scancode sc = SDL_GetScancodeFromName(v);
            if (sc != SDL_SCANCODE_UNKNOWN) bound[a] = sc;
        }
    }
}

static void binding_set(int a, SDL_Scancode sc)
{
    bound[a] = sc;
    char k[48]; snprintf(k, sizeof k, "key_%s", game->actions[a].key);
    eng_cfg_set(k, SDL_GetScancodeName(sc));
    fprintf(stderr, "[INPUT] %s = %s (saved)\n", game->actions[a].label, SDL_GetScancodeName(sc));
}

static bool key_held(const uint8_t *k, int a)
{
    const SDL_Scancode alt = game->actions[a].alt;
    return k[bound[a]] || (alt != SDL_SCANCODE_UNKNOWN && k[alt]);
}

static void on_key(SDL_Scancode sc, void *u)
{
    const int a = (int)(intptr_t)u;
    rebinding = -1;
    if (sc != SDL_SCANCODE_UNKNOWN) binding_set(a, sc);
}

/* ---- controller / raw-device discovery ---------------------------------- */
typedef struct { SDL_GameController *gc; SDL_JoystickID id; } pad_dev;
static pad_dev pads[MAX_DEV];

static int pad_find(SDL_JoystickID id)
{
    for (int i = 0; i < MAX_DEV; i++) if (pads[i].gc && pads[i].id == id) return i;
    return -1;
}
static int raw_find(SDL_JoystickID id)
{
    for (int i = 0; i < MAX_DEV; i++) if (raws[i].js && raws[i].id == id) return i;
    return -1;
}

static void pad_scan(void)
{
    for (int i = 0; i < MAX_DEV; i++)
        if (pads[i].gc && !SDL_GameControllerGetAttached(pads[i].gc)) {
            SDL_GameControllerClose(pads[i].gc); pads[i].gc = NULL; pads[i].id = -1;
        }

    for (int j = 0, n = SDL_NumJoysticks(); j < n; j++) {
        if (!SDL_IsGameController(j)) continue;
        const SDL_JoystickID id = SDL_JoystickGetDeviceInstanceID(j);
        if (pad_find(id) >= 0) continue;
        int slot = -1;
        for (int i = 0; i < MAX_DEV; i++) if (!pads[i].gc) { slot = i; break; }
        if (slot < 0) break;
        SDL_GameController *c = SDL_GameControllerOpen(j);
        if (!c) continue;
        pads[slot].gc = c; pads[slot].id = id;
        fprintf(stderr, "[INPUT] gamepad %d: %s\n", slot, SDL_GameControllerName(c));
    }
}

static void raw_scan(void)
{
    for (int i = 0; i < MAX_DEV; i++)
        if (raws[i].js && !SDL_JoystickGetAttached(raws[i].js)) {
            SDL_JoystickClose(raws[i].js); memset(&raws[i], 0, sizeof raws[i]); raws[i].id = -1;
        }

    for (int j = 0, n = SDL_NumJoysticks(); j < n; j++) {
        if (SDL_IsGameController(j)) continue;
        const SDL_JoystickID id = SDL_JoystickGetDeviceInstanceID(j);
        if (raw_find(id) >= 0) continue;
        int slot = -1;
        for (int i = 0; i < MAX_DEV; i++) if (!raws[i].js) { slot = i; break; }
        if (slot < 0) break;
        SDL_Joystick *js = SDL_JoystickOpen(j);
        if (!js) continue;
        raws[slot].js = js; raws[slot].id = id;
        const SDL_JoystickGUID guid = SDL_JoystickGetGUID(js);
        SDL_JoystickGetGUIDString(guid, raws[slot].guid, (int)sizeof raws[slot].guid);
        snprintf(raws[slot].name, sizeof raws[slot].name, "%s", SDL_JoystickName(js) ? SDL_JoystickName(js) : "joystick");
        fprintf(stderr, "[INPUT] raw %d: %s [%s], %d axes, %d buttons\n", slot, raws[slot].name, raws[slot].guid,
                SDL_JoystickNumAxes(js), SDL_JoystickNumButtons(js));
    }
}

static int raw_slot_for(const raw_axis_bind *b)
{
    if (b->guid[0]) {
        for (int i = 0; i < MAX_DEV; i++)
            if (raws[i].js && !strcmp(raws[i].guid, b->guid)) return i;
        return -1;
    }
    for (int i = 0; i < MAX_DEV; i++) if (raws[i].js) return i;
    return -1;
}

static bool raw_button_held(int a)
{
    if (a < 0 || a >= game->n || joy_button[a] < 0) return false;
    for (int i = 0; i < MAX_DEV; i++)
        if (raws[i].js && joy_button[a] < SDL_JoystickNumButtons(raws[i].js) &&
            SDL_JoystickGetButton(raws[i].js, joy_button[a])) return true;
    return false;
}

/* ---- Controls menu ------------------------------------------------------- */
static bool game_has_axis(int kind)
{
    for (int a = 0; a < game->n; a++) {
        const int x = game->actions[a].axis;
        if (kind == 0 && (x == SS22_AX_WHEEL_LEFT || x == SS22_AX_WHEEL_RIGHT)) return true;
        if (kind == 1 && x == SS22_AX_PEDAL1) return true;
        if (kind == 2 && x == SS22_AX_PEDAL2) return true;
    }
    return false;
}

static int axis_rows(void)
{
    int n = 0;
    for (int k = 0; k < 3; k++) if (game_has_axis(k)) n++;
    return n;
}

static int axis_kind_from_row(int r)
{
    int n = 0;
    for (int k = 0; k < 3; k++) if (game_has_axis(k)) {
        if (n == r) return k;
        n++;
    }
    return -1;
}

static void axis_capture_begin(int kind)
{
    axis_capture = kind;
    memset(cap_valid, 0, sizeof cap_valid);
    for (int d = 0; d < MAX_DEV; d++) if (raws[d].js) {
        const int na = SDL_JoystickNumAxes(raws[d].js) < MAX_CAP_AXES ? SDL_JoystickNumAxes(raws[d].js) : MAX_CAP_AXES;
        for (int a = 0; a < na; a++) {
            cap_base[d][a] = SDL_JoystickGetAxis(raws[d].js, a);
            cap_valid[d][a] = true;
        }
    }
}

static void axis_label(int kind, char *v, size_t vn)
{
    if (axis_capture == kind) {
        snprintf(v, vn, "%s", kind == 0 ? "move wheel..." : kind == 1 ? "press gas..." : "press brake...");
        return;
    }
    const raw_axis_bind *b = axis_bind(kind);
    if (b->axis < 0) { snprintf(v, vn, "(not bound)"); return; }
    const int d = raw_slot_for(b);
    if (d >= 0) snprintf(v, vn, "%s / axis %d%s", raws[d].name, b->axis, b->invert ? " (inv)" : "");
    else snprintf(v, vn, "axis %d%s", b->axis, b->guid[0] ? " (device disconnected)" : "");
}

static int nsw(void) { return (game->test_bit ? 1 : 0) + (game->service_bit ? 1 : 0); }
static int pg_n(void) { return nsw() + 1 + axis_rows() + game->n; }
static bool pg_val(int r) { (void)r; return false; }

static void pg_text(int r, char *l, size_t ln, char *v, size_t vn)
{
    *v = 0;
    if (game->test_bit && r == 0) { snprintf(l, ln, "Test mode"); snprintf(v, vn, "%s", test_latch ? "ON" : "OFF"); return; }
    if (game->service_bit && r == (game->test_bit ? 1 : 0)) { snprintf(l, ln, "Service button"); snprintf(v, vn, "press"); return; }
    r -= nsw();
    if (r == 0) { snprintf(l, ln, "Reset keyboard defaults"); return; }
    r--;
    const int ar = axis_rows();
    if (r < ar) {
        const int k = axis_kind_from_row(r);
        snprintf(l, ln, "%s axis", k == 0 ? "Wheel" : k == 1 ? "Gas" : "Brake");
        axis_label(k, v, vn);
        return;
    }
    const int a = r - ar;
    snprintf(l, ln, "%s", game->actions[a].label);
    const char *k = rebinding == a ? "press a key..." : SDL_GetScancodeName(bound[a]);
    snprintf(v, vn, "%s", (k && *k) ? k : "(none)");
}

static void pg_change(int r, int dir)
{
    if (dir != 0) return;
    if (game->test_bit && r == 0) { test_latch = !test_latch; fprintf(stderr, "[INPUT] test switch %s\n", test_latch ? "ON" : "OFF"); eng_ui_set_open(false); return; }
    if (game->service_bit && r == (game->test_bit ? 1 : 0)) { service_frames = 12; eng_ui_set_open(false); return; }
    r -= nsw();
    if (r == 0) {
        for (int a = 0; a < game->n; a++) if (bound[a] != game->actions[a].def) binding_set(a, game->actions[a].def);
        return;
    }
    r--;
    const int ar = axis_rows();
    if (r < ar) {
        const int k = axis_kind_from_row(r);
        axis_capture_begin(k);
        return;
    }
    rebinding = r - ar;
    axis_capture = -1;
    eng_ui_capture_key(on_key, (void *)(intptr_t)rebinding);
}

static void pg_notes(void (*line)(const char *fmt, ...))
{
    line("Wheel/pedals: select an axis row, then move that control.");
    line("Pedal direction is learned automatically and saved.");
    for (int i = 0; i < 3; i++) if (game->notes[i]) line("%s", game->notes[i]);
}

static const eng_ui_page controls_page = { "Controls", 520, 170, 22, pg_n, pg_val, NULL, pg_text, pg_change, pg_notes };
const eng_ui_page *ss22_input_page(void) { return &controls_page; }

/* ---- input lifecycle ----------------------------------------------------- */
static unsigned wheel = 0x200, pedal[2];

void ss22_input_init(const ss22_input_game *g)
{
    game = g;
    bindings_load();
    raw_bindings_load();
    SDL_SetHint(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, "1");
    SDL_InitSubSystem(SDL_INIT_GAMECONTROLLER);
    SDL_GameControllerAddMappingsFromFile("gamecontrollerdb.txt");
    pad_scan();
    raw_scan();
}

void ss22_input_event(const SDL_Event *e)
{
    if (e->type == SDL_JOYDEVICEADDED || e->type == SDL_CONTROLLERDEVICEADDED) {
        pad_scan(); raw_scan();
    } else if (e->type == SDL_JOYDEVICEREMOVED || e->type == SDL_CONTROLLERDEVICEREMOVED) {
        pad_scan(); raw_scan();
        memset(&gas_cal, 0, sizeof gas_cal); memset(&brake_cal, 0, sizeof brake_cal);
    }

    if (axis_capture < 0 || e->type != SDL_JOYAXISMOTION) return;
    const int d = raw_find(e->jaxis.which);
    const int a = e->jaxis.axis;
    if (d < 0 || a < 0 || a >= MAX_CAP_AXES || !cap_valid[d][a]) return;
    const int delta = (int)e->jaxis.value - cap_base[d][a];
    if (abs(delta) < 6000) return;

    raw_axis_bind *b = axis_bind(axis_capture);
    b->axis = a;
    b->invert = false;
    b->direction = axis_capture == 0 ? 0 : (delta > 0 ? +1 : -1);
    snprintf(b->guid, sizeof b->guid, "%s", raws[d].guid);
    raw_bind_save(axis_capture);
    if (axis_capture == 1) memset(&gas_cal, 0, sizeof gas_cal);
    if (axis_capture == 2) memset(&brake_cal, 0, sizeof brake_cal);
    fprintf(stderr, "[INPUT] %s bound to %s axis %d%s\n",
            axis_capture == 0 ? "wheel" : axis_capture == 1 ? "gas" : "brake",
            raws[d].name, a, b->direction > 0 ? " (+)" : b->direction < 0 ? " (-)" : "");
    axis_capture = -1;
}

static unsigned ramp(unsigned v, unsigned target, unsigned step)
{
    if (v < target) return v + step > target ? target : v + step;
    if (v > target) return v < target + step ? target : v - step;
    return v;
}

static bool raw_wheel_value(int *out)
{
    const int d = raw_slot_for(&joy_steer);
    if (d < 0 || joy_steer.axis < 0 || joy_steer.axis >= SDL_JoystickNumAxes(raws[d].js)) return false;
    int v = SDL_JoystickGetAxis(raws[d].js, joy_steer.axis);
    if (joy_steer.invert) v = -v;
    if (v > -WHEEL_DEADZONE && v < WHEEL_DEADZONE) v = 0;
    else {
        const int s = v < 0 ? -1 : 1;
        const int m = abs(v) - WHEEL_DEADZONE;
        v = s * (int)((long long)m * 32767 / (32767 - WHEEL_DEADZONE));
    }
    if (v < -32767) v = -32767;
    if (v >  32767) v =  32767;
    *out = v;
    return true;
}

static bool raw_pedal_value(raw_axis_bind *b, pedal_cal *cal, int *out)
{
    const int d = raw_slot_for(b);
    if (d < 0 || b->axis < 0 || b->axis >= SDL_JoystickNumAxes(raws[d].js)) return false;
    int r = SDL_JoystickGetAxis(raws[d].js, b->axis);
    if (b->invert) r = -r;

    if (!cal->rest_valid) {
        cal->rest = r;
        cal->rest_valid = true;
        cal->learned_direction = b->direction;
        if (!cal->learned_direction) {
            if (r > 12000) cal->learned_direction = -1;
            else if (r < -12000) cal->learned_direction = +1;
        }
    }
    if (!cal->learned_direction) {
        const int delta = r - cal->rest;
        if (delta > 4000) cal->learned_direction = +1;
        else if (delta < -4000) cal->learned_direction = -1;
        else { *out = 0; return true; }
    }

    const int dir = cal->learned_direction;
    const int delta = dir > 0 ? r - cal->rest : cal->rest - r;
    const int range = dir > 0 ? 32767 - cal->rest : cal->rest + 32768;
    double f = range > 0 ? (double)delta / range : 0.0;
    if (f < 0.02) f = 0.0;
    else f = (f - 0.02) / 0.98;
    if (f < 0.0) f = 0.0;
    if (f > 1.0) f = 1.0;
    *out = (int)(f * 32767.0 + 0.5);
    return true;
}

void ss22_input_update(void)
{
    const uint8_t *k = SDL_GetKeyboardState(NULL);
    uint16_t p = 0;
    int left = 0, right = 0, pk[2] = { 0, 0 };

    for (int a = 0; a < game->n; a++) {
        const ss22_action *ac = &game->actions[a];
        const bool held = key_held(k, a) || raw_button_held(a);
        if (ac->bit && ac->bit == game->test_bit) {
            if (held && !test_prev[a]) { test_latch = !test_latch; fprintf(stderr, "[INPUT] test switch %s\n", test_latch ? "ON" : "OFF"); }
            test_prev[a] = held;
        } else if (ac->bit && held) p |= ac->bit;
        if (held) switch (ac->axis) {
            case SS22_AX_WHEEL_LEFT: left = 1; break;
            case SS22_AX_WHEEL_RIGHT: right = 1; break;
            case SS22_AX_PEDAL1: pk[0] = 1; break;
            case SS22_AX_PEDAL2: pk[1] = 1; break;
            default: break;
        }
    }

    const int dir = right - left;
    const unsigned centre = (unsigned)((game->wheel_min + game->wheel_max) / 2);
    const unsigned wt = (unsigned)((int)centre + dir * game->wheel_key_span);
    wheel = ramp(wheel, wt, (unsigned)game->wheel_step);
    for (int i = 0; i < 2; i++)
        pedal[i] = ramp(pedal[i], pk[i] ? (unsigned)game->pedal_max[i] : 0, (unsigned)game->pedal_step);

    /* Standard gamepads. */
    bool stick = false;
    for (int i = 0; i < MAX_DEV; i++) {
        SDL_GameController *c = pads[i].gc;
        if (!c) continue;
        const int lx = SDL_GameControllerGetAxis(c, SDL_CONTROLLER_AXIS_LEFTX);
        if (lx > PAD_DEADZONE || lx < -PAD_DEADZONE) {
            const double t = ((lx < 0 ? -lx : lx) - PAD_DEADZONE) / (32767.0 - PAD_DEADZONE);
            wheel = (unsigned)((int)centre + (lx < 0 ? -1 : 1) * (int)(t * game->wheel_key_span));
            stick = true;
        }
        const int rt = SDL_GameControllerGetAxis(c, SDL_CONTROLLER_AXIS_TRIGGERRIGHT);
        const int lt = SDL_GameControllerGetAxis(c, SDL_CONTROLLER_AXIS_TRIGGERLEFT);
        if (rt > 2000) {
            const unsigned v = (unsigned)((double)rt * game->pedal_max[0] / 32767);
            if (v > pedal[0]) pedal[0] = v;
        }
        if (lt > 2000) {
            const unsigned v = (unsigned)((double)lt * game->pedal_max[1] / 32767);
            if (v > pedal[1]) pedal[1] = v;
        }
        for (int a = 0; a < game->n; a++) {
            const ss22_action *ac = &game->actions[a];
            if (!ac->bit || !ac->pad || ac->bit == game->test_bit) continue;
            for (int b = 0; b < SDL_CONTROLLER_BUTTON_MAX; b++)
                if ((ac->pad >> b & 1u) && SDL_GameControllerGetButton(c, (SDL_GameControllerButton)b)) { p |= ac->bit; break; }
        }
    }

    { static bool stick_drove;
      if (stick) stick_drove = true;
      else if (stick_drove) { stick_drove = false; if (dir == 0) wheel = centre; } }

    /* A raw wheel is absolute while it is moving; when centred, keyboard/pad remain usable. */
    { static bool raw_drove;
      int rw;
      if (raw_wheel_value(&rw)) {
          if (rw != 0) {
              const int negspan = (int)centre - game->wheel_min;
              const int posspan = game->wheel_max - (int)centre;
              int w = (int)centre + (rw < 0 ? (int)((long long)rw * negspan / 32767)
                                            : (int)((long long)rw * posspan / 32767));
              if (w < game->wheel_min) w = game->wheel_min;
              if (w > game->wheel_max) w = game->wheel_max;
              wheel = (unsigned)w;
              raw_drove = true;
          } else if (raw_drove) {
              wheel = centre;
              raw_drove = false;
          }
      } else raw_drove = false;
    }

    int rp;
    if (raw_pedal_value(&joy_gas, &gas_cal, &rp)) {
        const unsigned v = (unsigned)((long long)rp * game->pedal_max[0] / 32767);
        if (v > pedal[0]) pedal[0] = v;
    }
    if (raw_pedal_value(&joy_brake, &brake_cal, &rp)) {
        const unsigned v = (unsigned)((long long)rp * game->pedal_max[1] / 32767);
        if (v > pedal[1]) pedal[1] = v;
    }

    if ((int)wheel < game->wheel_min) wheel = (unsigned)game->wheel_min;
    if ((int)wheel > game->wheel_max) wheel = (unsigned)game->wheel_max;
    for (int i = 0; i < 2; i++) if ((int)pedal[i] > game->pedal_max[i]) pedal[i] = (unsigned)game->pedal_max[i];
    if (game->test_bit && test_latch) p |= game->test_bit;
    if (game->service_bit && service_frames > 0) { p |= game->service_bit; service_frames--; }
    game->send(p, wheel, pedal[0], pedal[1]);
}

void ss22_input_neutral(void)
{
    const unsigned centre = (unsigned)((game->wheel_min + game->wheel_max) / 2);
    wheel = centre; pedal[0] = pedal[1] = 0;
    game->send(game->test_bit && test_latch ? game->test_bit : 0, wheel, pedal[0], pedal[1]);
}
