# The desktop

zero boots into a desktop (set `desktop=off` in settings for the old text shell).

## Keys

| Keys | Action |
|---|---|
| Super+Space | search and launch an app |
| Super+Enter | terminal |
| Super+Q | close window |
| Super+Left / Right | snap to left / right half |
| Super+Up / Down | maximize / restore (minimize) |
| Super+F, Super+M | maximize, minimize |
| Super+T | toggle automatic tiling |
| Super+1..9 | open the n-th dock app |
| Alt+Tab | next window |

Drag a window to a screen edge to snap it (corners give quarters, the top edge maximizes).
In DOOM, click the window to capture the mouse; press the Super key to release it.

## Parts

- `sched.rs`: kernel threads, time slices, one thread per app.
- `gui/`: window manager and compositor (`desktop.rs`), apps (`apps.rs`, `files.rs`,
  `settings.rs`, `browser.rs`), drawing helpers (`draw.rs`).
- `net/`: e1000 driver, ARP/IPv4/ICMP/UDP/DHCP/DNS/TCP, HTTP client. Plain `http://` only (no TLS).
- `audio.rs`: AC97 driver + mixer (Doom source, UI and game effects, master volume).
- `smp.rs`: starts the other cores; `parallel_for` runs a loop on all of them.

Run with `.\run.ps1` (QEMU with 4 cores, sound, mouse grab on hover).
Shell extras: `net`, `ping <host>`, `fetch http://...`.
