eqoa: the vanilla EverQuest Online Adventures client, in Rust (dev build)

Source and documentation: https://github.com/DabDavis/eqoa-rust-client-vanilla

You need your own EverQuest Online Adventures: Frontiers disc image (SLUS-20744)
and an account on a server. Nothing of the game is in this download.

Run it from a terminal:

  Linux and macOS
    EQOA_ISO=/path/to/eqoa.iso ./eqoa

  Windows (Command Prompt)
    set EQOA_ISO=C:\path\to\eqoa.iso
    eqoa.exe

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

The game's options, your keys and the server are kept in eqoa-rs in the config
directory:
~/.config/eqoa-rs (Linux), ~/Library/Application Support/eqoa-rs (macOS),
%APPDATA%\eqoa-rs (Windows).

In play: Escape the start menu, F1 the keys and buttons (the Options window's
Controls page), M the map, F11 full screen. A controller works as the PS2's did.

Linux: needs ALSA (libasound2), libudev and a Vulkan or OpenGL driver.
macOS: the program is not signed. The first time, macOS refuses it; allow it in
System Settings, Privacy & Security ("Open Anyway"), or run
  xattr -d com.apple.quarantine eqoa
Windows: SmartScreen may ask the first time ("More info", "Run anyway").

This is a development build of master, replaced by the next one: only the
newest build can be downloaded.
