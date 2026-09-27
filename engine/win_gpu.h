/* win_gpu.h -- ask Windows to run the game on the DISCRETE graphics card of a laptop that has two (NVIDIA Optimus / AMD Switchable Graphics).
 *
 * Windows starts an OpenGL program on the INTEGRATED GPU unless the program is in the driver's own list or the user has set it up by hand
 * (Settings > System > Display > Graphics, or the NVIDIA Control Panel). A game built without these two exported variables therefore runs on the
 * iGPU of e.g. a Ryzen 5600H + RTX 3050 Ti laptop -- slower, and nobody chose it. The drivers look for exactly these names in the .exe's export
 * table; on a machine with one GPU they do nothing. Include this header ONCE per executable (the game's main file). */
#ifndef ENG_WIN_GPU_H
#define ENG_WIN_GPU_H
#ifdef _WIN32
__attribute__((dllexport)) unsigned long NvOptimusEnablement = 0x00000001;
__attribute__((dllexport)) int AmdPowerXpressRequestHighPerformance = 1;
#endif
#endif
