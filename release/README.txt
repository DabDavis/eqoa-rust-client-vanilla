eqoa: the vanilla EverQuest Online Adventures client, in Rust (dev build)

Source and documentation: https://github.com/DabDavis/eqoa-rust-client-vanilla

You need your own EverQuest Online Adventures: Frontiers disc image (SLUS-20744)
and an account on a server. Nothing of the game is in this download.

Run it from a terminal:

  Linux and macOS
    EQOA_ISO=/path/to/eqoa.iso EQOA_PASSWORD='your password' ./eqoa play <server> <account>

  Windows (Command Prompt)
    set EQOA_ISO=C:\path\to\eqoa.iso
    set EQOA_PASSWORD=your password
    eqoa.exe play <server> <account>

It logs in, takes the account's first character (EQOA_CHAR=<name> for another)
and opens a window on the world. There is no login or character screen in this
program yet: characters are made where the server makes them (its web page) or
on a PS2. A server on other ports: EQOA_AUTH_PORT and EQOA_GAME_PORT (9735 and
10070 by default).

The game's options and your keys are kept in eqoa-rs in the config directory:
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
