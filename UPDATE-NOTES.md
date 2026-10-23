# Update notes

What each dev build changed, for players, newest first: a section a build, added
in the same push as the changes it describes. The dev release's page shows them
(`release/notes.sh`, from `.github/workflows/dev.yml`); an edit to this file alone
is put on the page by `.github/workflows/notes.yml`.

## 2026-10-23 · The start menu in 4:3 again
- In a wide window the start menu (Start, or Escape) is the game's 4:3 picture
  again, in the middle with the world on either side, instead of stretching out
  to the window's edges. So are the inventory and the spell book.
- Its sides are closed off with the frame's studded posts, where on a TV they
  ran off the screen.
- The compass stays in view at the window's left while the menu is open.
- The buddy list and the mail's windows show the world behind them too, as
  every other window of the start menu does: their green backdrop no longer
  fills the screen and covers the compass.

## 2026-10-23 · Logging in inside, Spirit of the Wolf, invisibility
- You log back in where you logged out: inside a house, a stall or a cave you
  come back inside it, not on its roof or on the ground above the cave. Coming
  back from the dead, and arriving in another world, stand you where the
  server says too.
- Speed spells work: Spirit of the Wolf makes you run faster, and a snare
  slows you, by what the server says, as on a PS2 (on Sandstorm a run is
  8.8 and 11.5 under Spirit of the Wolf). Before, you always ran at the same
  speed.
- Invisibility shows: an invisible character's body is not drawn, yours too;
  what it holds, its helm and its name stay, as on a PS2.

## 2026-10-23 · Report a problem, and crash reports
- F3, or **Report a bug** under the target's bar, opens a box over the game
  (the screens before the world too): say what went wrong and press Send. A
  picture of the screen goes with it, and what the game knew at the time. Your
  note and the picture appear on the report board
  (eqoaclassic.com/eqoa/reports/board), with your name if you give one (Tab);
  the rest goes to the developer only. Your account and password are never
  sent.
- When the game crashes, or has to be killed, it asks at the next start
  whether to send what it wrote about it: Send, Not now or Delete.
- The game keeps a log, `eqoa.log`, in its settings folder.
- In the browser the same: F3, Report a bug, and the crash report when the page
  stops.

## 2026-10-23 · The game updates itself
- As it starts, the game looks for a newer build. When there is one it
  downloads it (the loading screen shows how far it is), checks it, and starts
  it in its place, before the title: every fix reaches you without another
  download.
- This is the last build you download by hand. The game needs to be able to
  write to its own folder; if it cannot, it plays as it is.
- `EQOA_NO_UPDATE=1` plays a build as it is.

## 2026-10-23 · Follow, guild chat, assist
- Follow works: target someone in your group and choose Follow in the quick
  chat's commands, or type `/follow`. You turn to them and run after them,
  slow to a walk when you are close and stop beside them, a little faster than
  a plain run so you keep up. Pushing forward or back, attacking, or them
  leaving the group ends it; following again ends it too.
- The chat mode's Guild sends what you type to your guild. It always said you
  were not in a guild.
- Assist Target targets what your target is fighting, and Assist Leader what
  your group's leader has targeted (the server has to answer them:
  eqoaclassic does from today).

## 2026-10-22 · Update notes on the release page
- The release page lists what each build changed, newest first, so the notes of
  a build stay readable after the next one replaces it.

## 2026-10-22 · Several windows at once, each with its own controller
- With several windows of the game open (multiboxing), each reads its own
  controller: the first window the first controller, the second the next,
  whichever window is in front. A window alone reads every controller, as before.
- The Options window's Extended page has **Controller Focus**, kept for each
  window: Auto (as above), Window (every controller drives the window in front),
  Pad 1 to 4 (that controller; windows set to the same one are all driven by it)
  and Off. Under it, what the window reads now.
- A controller that drops out (its battery, a wireless reconnect) comes back to
  its own window when it is back within 15 seconds.
- Windows: Xbox controllers, and those Steam Input or DS4Windows present as one,
  reach a window behind another; other controllers reach only the window in
  front, as Windows gives them.
- Windows: no console window opens beside the game.

## 2026-10-22 · Summons and resurrections
- A summons or a resurrection a server offers (eqoa.live does) puts up its
  Yes/No box, and the answer goes back. It was ignored before.

## 2026-10-22 · Linux and macOS notes
- README.txt says what Linux needs (glibc 2.34 or newer: Ubuntu 22.04, Debian 12,
  Fedora 35 or later) and how macOS lets the unsigned program run (Privacy &
  Security's Open Anyway, or `xattr`).

## 2026-10-22 · Windows: the game runs
- On Windows the game showed a white window and closed before it drew anything.
  It runs now.

## 2026-10-22 · A swapped weapon shows
- A weapon put on, taken off or swapped shows in the hand at once, yours and
  everyone's; the old one stayed until the character left view.

## 2026-10-22 · The disc in a file dialog, the menus' sounds
- The first time, the game asks for your disc image in the desktop's own file
  dialog (any name, any folder) and keeps it for the next time.
- The menus' clicks and keys are heard, and the title's music plays.
- A player in a creature's form holds no weapons.
- The window draws at the display's rate (vsync).

## 2026-10-22 · A wide title screen
- The title screen and Station's pages fill a wide window.
- No black line at the stern of the title's ship.

## 2026-10-22 · The game from its boot
- The game starts as the PS2 does: the loading screens, the legal screen and the
  licence, then the title with Play, Network Setup and Options.
- Network Setup on the title takes the server's address and keeps it.
- Play brings Station's login screen (Save Account keeps the account and the
  password), the server's news, the server list, character select and character
  creation, then the world. Quit in the start menu goes back to the server list.
- In a browser the page asks only for the disc image; you sign in on Station's
  screen.

## 2026-10-22 · The program alone
- Beside each archive, the program can be downloaded on its own (on Linux and
  macOS, `chmod +x` it after the download).

## 2026-10-22 · The first build
- EverQuest Online Adventures: Frontiers for Linux, Windows and macOS, from your
  own disc image: the game as the PS2 plays it, in a wide window, at a frame rate
  free of the game's 30, with the mouse and the touch screen, the keyboard (WASD
  walks) and the options' Controls page, 1,000 lines of chat, the Extended options
  page and the map.
