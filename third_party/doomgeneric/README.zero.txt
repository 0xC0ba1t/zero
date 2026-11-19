doomgeneric (https://github.com/ozkl/doomgeneric), a portable port of the id Software
DOOM source release. Licensed under the GNU GPL v2 (see LICENSE). Only the files the
Zero kernel build uses are kept here; the platform layer is written in Rust
(kernel/src/doom) instead of one of the upstream doomgeneric_*.c backends.
zero patch: m_config.c has its float support removed (the kernel has no FPU state).
zero patch: i_system.c I_Quit() calls exit(0) so the engine returns to the shell.
zero patches (sound): i_zerosound.c is new (effect mixer + OPL2 FM emulation + MUS player); i_sound.c and
i_sound.h use it unconditionally instead of the SDL_mixer based modules. i_input.c gains mouse input.
zero patch: m_controls.c restores the original default keys (Ctrl fire, Space use, comma/period strafe).
