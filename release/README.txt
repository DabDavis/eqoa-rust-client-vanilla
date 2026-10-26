eqoa: the vanilla EverQuest Online Adventures client, in Rust (dev build)

Source and documentation: https://github.com/DabDavis/eqoa-rust-client-vanilla

You need your own EverQuest Online Adventures: Frontiers disc image (SLUS-20744)
and an account on a server. Nothing of the game is in this download.

Start eqoa (eqoa.exe on Windows; ./eqoa on Linux and macOS). The first time,
it asks for your disc image in a file dialog: it may have any name and be in
any folder. It checks that the image is Frontiers and keeps its place for the
next time, so after that eqoa starts straight away (if the image is moved or
deleted, it asks again).

To change the image, start eqoa with the new one's path,
  eqoa /path/to/image.iso
or on Windows drop the image on eqoa.exe: that one is kept from then on.
EQOA_ISO=/path/to/image.iso names one for a single run.

The game starts as the console does: the loading screens, the legal screen, the
licence, the title. On the title, Network Setup takes the address of the server
to play on (a host name or an IPv4 address; it is kept). Play brings Station's
login screen: your account and password (Create User Name makes an account where
the server allows it; Save Account keeps them, the password in plain text, as the
memory card did). Then the server's news, the server list, character select (an
empty slot makes a new character) and the world. Quit in the start menu goes
back to the server list.

For scripts: eqoa play <server> <account> (password in EQOA_PASSWORD,
EQOA_CHAR=<name> for a character other than the first) goes straight into the
world. A server on other ports: EQOA_AUTH_PORT and EQOA_GAME_PORT (9735 and
10070 by default), EQOA_NEWS_PORT (7000).

The program's icon is a drawn disc until it has read your disc image; from then
on it is the game's own memory card icon, drawn from your disc (kept as
saveicon.png beside your options, for the next start). On Linux, eqoa.png in
this archive is the drawn disc, for a desktop entry of your own (Wayland shows a
program's icon only through one).

The game's options, your keys, the server and the disc image's place (disc.opt,
a text file holding its path) are kept in eqoa-rs in the config directory:
~/.config/eqoa-rs (Linux), ~/Library/Application Support/eqoa-rs (macOS),
%APPDATA%\eqoa-rs (Windows).

In play: Escape the start menu, F1 the keys and buttons (the Options window's
Controls page), M the map, F11 full screen. A controller works as the PS2's did.

Several windows of eqoa at once (multiboxing): each reads its own controller,
the first window the first controller, the second the next, whichever window
is in front. Options, Extended, Controller Focus picks another for a window
(windows on the same one are all driven by it), Window (the controllers follow
the window in front) or Off. On Windows a controller that is not an Xbox one
(or passed off as one, by Steam Input or DS4Windows) reaches only the window in
front: Windows gives it to no other.

Linux: needs glibc 2.34 or newer (Ubuntu 22.04, Debian 12, Fedora 35 or later),
ALSA (libasound2), libudev and a Vulkan or OpenGL driver. The file dialog is
your desktop's (its XDG desktop portal, or zenity); with neither, start it with
the image's path as above.
macOS: the program is not signed. The first time, macOS refuses it; allow it in
System Settings, Privacy & Security ("Open Anyway"), or run
  xattr -d com.apple.quarantine eqoa
Windows: SmartScreen may ask the first time ("More info", "Run anyway").
eqoa.exe opens no console window; started from a terminal, its messages print
there.

This is a development build of master, replaced by the next one: only the
newest build can be downloaded.

The game updates itself: as it starts, it asks the release for a newer build,
and when there is one it downloads it (the loading screen says so), checks it is
the file the release names, puts it in its own place and starts it, before the
title. It needs to be able to write to its own folder; if it cannot, or the
download fails, it says so in its log and plays as it is. EQOA_NO_UPDATE=1
leaves it as it is for a run.
