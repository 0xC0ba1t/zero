# The desktop

zero boots into a macOS-style desktop (set `desktop` to `off` in settings for the plain text shell).
The first time, an animated setup asks your name, light or dark, an accent colour and a wallpaper.

## Around the screen

- **Menu bar:** the zero menu (About, Settings, Launchpad, Mission Control, Lock, Restart, Shut Down),
  the app's own menus, and on the right Spotlight, Control Centre (dark mode, tiling, focus, Bit,
  volume), network and the clock (click it for the calendar and notifications).
- **Dock:** magnifies, bounces apps as they open, shows running apps with a dot; right-click an
  icon for its menu; drop a file on the trash. Minimizing flies windows into the dock.
- **Desktop icons:** double-click to open, drag to move, right-click for a menu, drop on windows.
- **Bit:** walks along the dock. Click to pet, double-click to chat. It learns (see Settings > Bit).

## Keys

| Keys | Action |
|---|---|
| Super+Space | Spotlight: apps, files, sums (`2+3*4`), ask Bit |
| F3 / top-left corner | Mission Control |
| F4 | Launchpad |
| Super+Tab / Alt+Tab | switch windows |
| Super+Enter, Super+E, Super+, | Terminal, Files, Settings |
| Super+W, Super+Q | close window, quit app |
| Super+M / Super+H, Super+F | minimize, zoom |
| Super+Left / Right | snap to half; Super+Up / Down zoom / restore |
| Super+T | automatic tiling |
| Super+D | show desktop |
| Super+1..8 | open the n-th dock app |

Drag a window to an edge to snap it (corners give quarters, the top zooms). In DOOM, click the
window to capture the mouse; press Super to release it.

## Apps

- **Files:** icon and list views, sidebar, search, Get Info, rename (F2), duplicate, copy/paste,
  drag onto folders, a Trash with Put Back, picture thumbnails, right-click menus everywhere.
- **Media:** PNG, GIF (animated), BMP and PPM pictures (zoom, pan, rotate, slideshow, set as
  wallpaper) and WAV music with a waveform player.
- **Notes:** a text editor; save, save as, run `.zs` files.
- **Browser:** plain HTTP pages; picture and sound links are downloaded and opened in Media.
- **Settings:** General, Appearance, Desktop & Dock, Wallpaper, Displays, Sound, Network, Bit,
  Keyboard, Date & Time, Storage, Users.
- **Games and tools:** Snake, Pong, Space Invaders, Tic Tac Toe, Connect Four, Calculator, Bounce
  (all written in ZeroScript, see `zs.md`), Doom, Terminal, Activity.

## Under the hood

`sched.rs` threads, `gui/gfx.rs` anti-aliased drawing, `gui/desktop.rs` window manager and
compositor (drawn on all cores), `gui/ui.rs` the toolkit apps use, `zs.rs` the language,
`codecs.rs` PNG/GIF/BMP/WAV, `net/` TCP/IP, `audio.rs` mixer, `smp.rs` extra cores.
Run with `.\run.ps1` (QEMU with 4 cores, sound and mouse grab).
