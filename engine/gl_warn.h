/* gl_warn.h -- say so, loudly, when the OpenGL renderer is SOFTWARE: the game then runs at a fraction of its speed and the log is where the player (or a bug
 * report) finds out why. Called once with glGetString(GL_RENDERER) right after the context exists. */
#ifndef ENG_GL_WARN_H
#define ENG_GL_WARN_H
#include <stdio.h>
#include <string.h>
static inline void eng_gl_warn_software(const char *renderer)
{
    static const char *const soft[] = { "llvmpipe", "softpipe", "swrast", "GDI Generic", "Microsoft Basic Render", "SwiftShader", "Software Rasterizer", NULL };
    if (!renderer) return;
    for (int i = 0; soft[i]; i++)
        if (strstr(renderer, soft[i])) {
            fprintf(stderr, "[GL] WARNING: \"%s\" is a SOFTWARE renderer: the graphics card is not being used and the game will be slow. "
                            "Install your graphics driver, or on a laptop with two graphics chips make sure the game runs on the fast one (README: \"Slow?\").\n", renderer);
            return;
        }
}
#endif
