# Update notes

What each dev build changed, for players, newest first: a section a build, added
in the same push as the changes it describes. The dev release's page shows them
(`release/notes.sh`, from `.github/workflows/dev.yml`); an edit to this file alone
is put on the page by `.github/workflows/notes.yml`.

## 2026-10-23 · Outmatched, background sound, a Mac Settings item
- Attacking something far beyond you gets the game's red "You are outmatched.
  Attacking again is not advised." and your attack stops, as on a PS2. Before,
  nothing happened and the attack stayed on.
- Options > Extended has Sound in Background: turn it Off and a game window
  that isn't in front goes quiet. It's one setting for all your windows, and
  the sound picks up again when you switch back.
- On a Mac, Settings… (⌘,) in the app menu opens the game's Options.
- With Delete Character highlighted on Character Selection, the dark panel
  fills the character view's frame exactly, in any window size, with nothing
  of the room showing at its side.
- On Character Selection and character creation, a character without a weapon
  moves from the start, with its first idle fidget as on a PS2, instead of
  standing still for five seconds.
- A problem report says what the game's sound is playing and at which volumes.

## 2026-10-23 · Sounds no longer stack
- Sounds no longer play several times on top of each other (spell casts, the
  hits of a double or quad attack). The change that gave each hit of a quad
  attack its own sound is taken back until it plays as on a PS2: for now only
  a quad attack's first hit makes the sound, as before.

## 2026-10-23 · Server updates and patches, the game's camera and fog
- Play checks your server for updates first, as on a PS2: the Update window
  shows the progress, the server's update messages open when something
  changed, and Play at Done goes on to the login. Each server's update files
  are kept in that server's own folder, so switching servers never mixes
  them. If a server's update service doesn't answer, the game plays with the
  files that server sent last time and says so.
- On a server that sends its own patch, such as Sandstorm, the game plays as
  that patch makes it: its 30 armour colours, armour set 9 with its helm,
  robes 4 and 5, coloured damage numbers, the auction search up to level 60,
  and Search, Register and Unregister in the Group menu.
- The world is seen through the game's own camera, as on a PS2: you see more of
  it, and the start menu's picture of your character shows all of it, head
  included.
- The game's fog: the world ends where the fog does, so far hills, trees and
  buildings fade into the sky instead of standing out as pale shapes. Options
  > Extended has a Fog slider (the game's in the middle, off at the left,
  thicker to the right); the Extended page now scrolls a row.
- The time of day follows the server's clock, so your sky, light and fog match
  everyone else's. Before, every game started at ten in the morning.
- The start menu, its windows and the merchant's window end at their bottom
  frame: the green strip below it is gone and the world shows there.
- The Who list and the guild's roster are sorted by name; ○ (C on the
  keyboard) or a click on a column's heading sorts them by another column.
- In a browser the update check and a server's own patch aren't there yet: the
  page plays the original game as before.

## 2026-10-23 · Options per window, destroy asks, weapon swaps in step
- Each game window keeps its own options: turn the sound off in one and keep it
  in another. The first window keeps the options you already have, and a new
  window starts from them. Each window remembers its own saved account; the
  keys you set on the Controls page are shared by every window.
- Destroying an item always asks first ("Destroy item?"); before, only an
  equipped, prepared or NO TRADE item did, as on a PS2.
- Switch weapons in the middle of a fight, a bow to a sword say, and your swing
  stays in step with the damage numbers instead of drifting apart.
- Gaining a Class Mastery point opens the game's own box ("You have earned 1
  mastery point."), as on a PS2, instead of a line in the chat box.
- A dead creature's target bar no longer shows its face icon.
- With the merchant, the bank or a trade open, the world shows under the chat
  box and the target instead of a green strip.
- The problem report (F3) can leave the picture out: "Send the picture of the
  screen" under your name; Tab then Space, or a click.

## 2026-10-23 · Looting, auction Inspect, the spell book, creatures' own effects
- Looting works: the item you take goes into your inventory and off the corpse
  at once. Before, it stayed on the corpse until you logged in again, and
  taking it a second time said "item not found". A corpse that won't open now
  says why.
- In the auction search, Inspect on a lot shows its full description: damage,
  slot, hit points, who can use it, its stats and its text.
- The spell book looks like the start menu: the game's band across its top
  behind the level tabs, and its side frames running all the way up the screen.
- Creatures with effects of their own show them: the necromancer's Red
  Swarming Death is a swarm, and a Flesh Fiend has its flies.

## 2026-10-23 · What's new in the update question, every hit heard
- When a new version is ready, the game's question shows what's new in it: the
  update notes of every version since yours. Scroll them with the d-pad, a
  stick, the arrow keys, the mouse wheel or by dragging the list. In a browser,
  the new-version notice lists what's new too.
- Every hit of a double or quad attack makes its sound, not just the first,
  whoever is swinging.
- Training points the server gives you, such as the rehabilitator's refund,
  show at once with the game's "You have received more training points!", no
  logging in again needed.
- In Rathe, Odus and the other worlds, the start menu's Location names the
  place you're in, not one in Tunaria.
- The map's arrow for you is smaller, and it stays the same size whether you
  zoom in or out.
- Options > Extended has Screen Shake: turn it Off and spells never shake the
  view.
- Options > Extended has Capture Controller: turn it On and a controller that
  several of your windows share (the same Pad in each) moves only the window in
  front. Alt-Tab or click to switch windows; setting it in one window sets it
  in all of them. What the window reads is now said beside Back.

## 2026-10-23 · Auction lists, a restart notice, camera turn speed
- The auction search's Class, Race, Equip Type and Att. Type open their lists
  again: pick one with ✕ or a click and the search looks only for it. The
  search's rows are the game's black on parchment, the selected one blue.
- When a new version comes out while you're playing, the game tells you in its
  own message box and asks whether to restart now. It works on any server,
  Sandstorm included. Yes logs you out cleanly and starts the new version (sign
  in again); No keeps you playing, and it asks again in half an hour. The box
  starts on No and never comes up while you're typing, fighting or in a menu.
- In a browser, "Later" on the new-version notice asks again after half an hour.
- Options > Extended has a Camera Turn Speed slider: how fast the controller's
  stick and the keys turn the view, from half to three times the game's speed.
  The mouse is unchanged.
- The window's title is "EQOA 1", "EQOA 2" and so on by window, with your
  character's name once you're in the world, so several windows are easy to
  tell apart.
- Character Selection and character creation's race page have their left border
  in a wide window, as they do on the right.
- Spell effects no longer erase the names over people's heads. Names are drawn
  where the game draws them, so a spell's glow shines over them as on a PS2.
- Glowing weapons glow: the Sword of Virtue, the Nitefyre and Night's Point
  blades, and the bright staves, wands and totems.
- A creature that dies while walking falls down at once instead of walking on.
- Problem reports include what each nearby character is wearing, as the server
  sent it.

## 2026-10-23 · The merchant in 4:3 too
- In a wide window a merchant's buy and sell window is the game's 4:3 picture
  too, in the middle with the world on either side, its sides closed off with
  the frame's studded posts, as the start menu's are.
- The compass stays in view while you shop.

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
