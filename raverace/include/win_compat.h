/* win_compat.h -- the few POSIX pieces Rave Racer uses, mapped onto Windows.
 * Force-included into every source file by CMakeLists.txt when building for
 * Windows (MinGW-w64); the Linux build never sees it. Shared POSIX mappings
 * live in ../../include/win_compat.h. */
#ifndef RR_WIN_COMPAT_H
#define RR_WIN_COMPAT_H
#ifdef _WIN32
#include "../../include/win_compat.h"

/* The Win32 headers define this obsolete memory-model keyword; the lifted
 * game code uses 'near' as a normal variable name. */
#ifdef near
#undef near
#endif

/* sysconf(_SC_NPROCESSORS_ONLN): the renderer's thread count -- SDL knows it */
#ifndef _SC_NPROCESSORS_ONLN
#define _SC_NPROCESSORS_ONLN 84
#endif
extern int SDL_GetCPUCount(void);
static inline long rr_win_sysconf(int name) { (void)name; return SDL_GetCPUCount(); }
#define sysconf rr_win_sysconf

#endif /* _WIN32 */
#endif /* RR_WIN_COMPAT_H */
