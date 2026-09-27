/*
 * eng_ffb.h -- a cabinet's steering motor on a force-feedback wheel (engine/eng_ffb.c, SDL haptic).
 *
 * Namco's driving cabinets of the mid-90s (Rave Racer, Dirt Dash, Ace Driver) drive the wheel through a
 * Motor/Feedback PCB that takes one byte a frame: bit 1 = the direction (set: toward the wheel's higher
 * A-D side), bits 2-7 = 63 - the strength with the bit order reversed; 0xFF = no force. Bit 0 differs
 * between the games (Dirt Dash sets it, Rave Racer does not) and is ignored. Strength 63 (bits 2-7 all
 * clear, e.g. 0x00 before the game has written anything) is never sent and reads as no force.
 *
 * The force is one SDL constant force on the device the host says the steering axis is bound to.
 */
#ifndef ENG_FFB_H
#define ENG_FFB_H
#include <stdbool.h>
#include <stdint.h>
#include <SDL2/SDL.h>

int  eng_ffb_decode(uint8_t b);                 /* -63..63: negative pushes toward the higher A-D side */
/* the steering device (NULL = none), every frame or on a change: opened once per device, again after a
 * hot-plug or a rebind. Call eng_ffb_forget(id) BEFORE closing a joystick. */
bool eng_ffb_device(SDL_Joystick *js);
void eng_ffb_forget(SDL_JoystickID id);
/* play a decoded command: strength 0-100 %, reverse = push the other way (an inverted steering axis
 * XOR the user's FFB direction setting) */
void eng_ffb_force(int motor, int strength, bool reverse);
void eng_ffb_close(void);                       /* stop the force and let go of the device (also run at exit) */
#endif
