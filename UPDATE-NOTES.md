# Update notes

What each dev build changed, for players, newest first: a section a build, added
in the same push as the changes it describes. The dev release's page shows them
(`release/notes.sh`, from `.github/workflows/dev.yml`); an edit to this file alone
is put on the page by `.github/workflows/notes.yml`.

## 2026-10-24 · The extra ability bars are back
- Options > Extended has Ability Bars again: 2 (the game's), 3 or 4. With 3
  or 4, the HUD has a third and a fourth ability bar, five abilities each,
  after the second bar when you switch bars, and the spell book's Memorize
  offers their slots too (ten at a time; moving past the tenth shows the
  next ten).
- They go with the weapon sets: on eqoaclassic, pressing right goes through
  your ability bars, then the items, then the weapon sets.
- The server keeps which ability is on which slot: eqoaclassic keeps the
  extra bars, Sandstorm keeps only the game's two. If the server does not
  take an ability onto bar 3 or 4, the chat says so in red.

## 2026-10-24 · SOE's final toolbars: weapon sets, two potions and two gems
- On a server that keeps weapon sets (eqoaclassic does), the HUD's menus are
  the ones the live game had at its end. Pressing right on the d-pad goes
  from your two ability bars to the items, then to a new fourth menu: four
  weapon sets of your own and "bare hands". Press X on a set to take its
  weapons up in one go; bare hands puts away everything in your hands.
- Making a set: in Items, pick a weapon on the Equipment tab (a sword, an
  axe, a bow, a staff, a fishing pole or a tool) and choose Prepare. If you
  are not holding it, you take it up first. Pick one of the four sets with up
  and down, press X and type a name (up to 15 letters). A set remembers both
  your hands as they are at that moment, so hold what you want in your off
  hand (a second weapon or a shield) before you prepare it.
- The item quick slots are the live game's too: two potions and two gems
  (stones, wands, costumes and other items you use), where the disc had one
  gem and a weapon.
- On other servers the menus stay as the disc has them.
- Options > Extended no longer has Ability Bars: the extra third and fourth
  ability bars are gone for now, and the spell book offers the game's ten
  slots again.

## 2026-10-24 · The weather after a port
- After a teleport, a coach ride or coming back to life, the sky now turns
  at once to the sky of where you arrive, as on a PlayStation 2, instead of
  fading over half a minute. A change of weather the server sent meanwhile
  waited behind that fade, so a character who travelled a lot could stay a
  change or more behind one who stood still: two windows side by side could
  show different weather.
- A problem report (F3) now says what weather each window shows, so two
  windows' reports can be compared.

## 2026-10-24 · Each window opens where you left it, and VSync
- Each game window now opens as you last had it: its size, where it was on
  your desktop, its monitor, and maximized or full screen (F11). With several
  windows open, each keeps its own. A window closed while minimized opens as
  it was before.
- No more screen tearing: the game now waits for your display's refresh
  before it shows a frame. Options > Extended > VSync (its last row) turns
  that off for the least delay, tearing allowed, still no faster than your
  display.

## 2026-10-24 · Controllers: the map on a button, PlayStation pads behind other windows
- Options > Controls: a controller's button can now be put on any of the
  keyboard's own rows: the map, its zoom, targets 1 to 6, the group keys, F9's
  art and the rest. That button then does what the key does (R3 on the map:
  R3 opens and closes the map) instead of what it did in the game, and the
  page says which row lost it. Defaults puts it all back.
- On Windows a DualShock 4 or a DualSense now works in a game window that is
  not in front, as an Xbox controller already did: with two windows open,
  each can have its own controller, PlayStation or Xbox. Over USB, Bluetooth
  or the DualShock 4's wireless adapter. Before, Windows gave a PlayStation
  controller only to the window in front. On Linux and on a Mac every window
  already read every controller.
- A problem report (F3) now lists the controllers connected, how each is read
  and which game window reads it.

## 2026-10-24 · Sandstorm: stacks at auction, guilds of 255
- On Sandstorm, gems, crafting materials and anything else that stacks can
  now be put up for auction from the bank, as Sandstorm's own patch lets a
  PlayStation 2 do. The original game refuses anything that stacks.
- On Sandstorm a guild can invite members until it has 255, as its patch
  allows; the original game stops at 128.

## 2026-10-24 · Fixes from your reports
- Holding Tab (or `) to aim and switching to another window no longer leaves
  the aim stuck when you come back: whatever you held as the window lost the
  keyboard is let go. The same for C's chat history, Q's target window and
  Space's attack.
- Pressing Escape while aiming, or while holding C for the chat history, no
  longer closes the game.
- The spell book's right column has its ten plates again, one behind each
  memorized ability's name, as on a PlayStation 2; the tell, mail and
  help-request forms show the boxes their fields are typed into, and a trade
  the boxes of its tunar.
- No more dark line in the start menu's title plate (beside "Group", among
  others) at some window sizes.
- The sky no longer jumps while it changes (rain coming or going, or another
  region's sky): the new sky fades in with the same drift as the old one.
- With NPC/PC View Distance raised, creatures and players far off in the fog
  move as smoothly as the ones near you.
- With the camera tilted down to your head's height or lower (the mouse's
  tilt), a slope or a wall behind you no longer swings the camera round, and
  your run with it: the camera comes in close behind you instead.
- A long teleport within the same world no longer leaves you under the
  ground when the place it sends you to is a little below the floor.
- Changing form back and forth (Mouse Form, Remove Illusion, any illusion)
  no longer takes a little more memory each time.
- A game that ended because no disc image was chosen is no longer offered as
  a crash at the next start.

## 2026-10-24 · A window you cannot see no longer fills the memory
- A game window that cannot be seen (behind another window, on another
  full-screen space, minimized) now keeps playing at the game's own 30 frames a
  second, using almost no processor, and no longer takes more memory every
  frame. On a Mac, two windows one behind the other could fill the memory until
  the computer froze.

## 2026-10-24 · Players, NPCs and signposts are solid
- You no longer walk through other players, NPCs or signposts: you stop at
  them, or slide round them when you are not heading straight at them, as on a
  PS2. Corpses still let you through, and so do players on your ignore list.

## 2026-10-24 · The model viewer turns at once
- In View offline, F9's 2002 model now shows as soon as the disc is read,
  not only at the next key pressed.

## 2026-10-24 · The model viewer follows F9
- In the page's View offline (and `eqoa view`), F9 now turns the model shown to
  the 2002 disc's and back; before, it read the disc but kept showing
  Frontiers' model.

## 2026-10-24 · The 2002 art: F9
- If you have the original EverQuest Online Adventures disc of 2002 (the USA
  one, SLUS-20470) as an image beside your Frontiers one, press F9 and the game
  is drawn as it looked before Frontiers: every race's body, face and hair, the
  armour, helms and robes, the creatures that game had, the windows' blue
  stone, the item icons, the spell effects and the sky. F9 again brings
  Frontiers back, at once, wherever you are.
- It works before the world too: the loading pictures, the legal screen, the
  title's painting and its music, Station's pages, the server list, character
  select and creation.
- Tunaria comes from that disc as well, with its own lighting and props, and
  keeps Frontiers' fog, dust, falling leaves, birds and ambient sounds.
  Frontiers' other worlds, and the creatures and icons Frontiers added, stay as
  they are.
- The first time, press F9 at the title or at character select: it asks for
  the disc image and remembers it, and the game remembers which art you chose.
  Options > Extended > Art does the same as F9, for a controller, and F9 can
  be moved on the Controls page.
- Without that disc nothing changes and the game plays as before. F9 in the
  world then only says, in the chat, where to choose the disc.
- Only the pictures change: the game, its words and its rules are Frontiers',
  on any server.
- In the browser: pick the 2002 disc as the page's second image, or press F9 in
  the game and pick it then.

## 2026-10-24 · Arrows fly
- A shot with a bow or a crossbow now shows its arrow or bolt flying from the
  hand to the target in a slight arc, as on a PlayStation 2, and a thrown
  weapon flies itself. Before, nothing left the hand.
- Your own shots show it, and so do other players' and creatures' when the
  server shows you their shots. As on the console, nothing is seen to fly
  when the archer also holds something in the main hand.

## 2026-10-24 · Hold R1 to aim
- Holding R1 or L1 (Tab or ` on the keyboard) for a moment now does what it
  does on a PlayStation 2: the view moves into your character's eyes, a
  crosshair appears in the middle of the screen, and whoever is nearest the
  crosshair becomes your target. A quick press still just picks the next
  target, as before.
- While you hold it, the right stick looks around (I J K L on the keyboard,
  or drag with the mouse or a finger), and your character turns to follow
  your look. You stand still meanwhile, as on the console, but the quick
  menu's up, down and use still work: aim at someone and cast at them.
- Let go and the camera goes back to its usual place, with your character
  facing where you looked. Opening the chat line ends it too.

## 2026-10-24 · More NPCs and players in view, and farther
- Options > Extended has two new rows. NPC/PC Display, a slider from 24 (the
  game's) up to 204, is how many NPCs and players, you among them, the game
  keeps in view at once: the game always showed only the nearest 24, so in a
  crowded town the rest were missing. NPC/PC View Distance, a slider from 100
  (the game's) up to 1000, is how far away they are drawn: the game stops
  drawing a player race's body (most townsfolk) 100 away.
- Who is in view is the server's choice, so NPC/PC Display works only on a
  server that offers more: eqoaclassic does, up to 204 within 1000. Sandstorm
  and eqoa.live keep the game's 24. The view distance draws whatever the
  server sends that far on any server. In the world, with either row chosen,
  the line at the bottom of the page says what this server gives you.
- Both raise what there is to draw: on a slower computer, or in a browser,
  lower numbers keep the game smoother. Characters farther away than the game
  would draw them move their arms and legs a little less often, which you
  will not see at that distance.

## 2026-10-24 · The sound comes back by itself
- If the sound device goes away while the game runs (headphones unplugged, the
  computer put to sleep and woken, the system's sound restarted), the game now
  finds the device that takes its place and the sound comes back by itself,
  where the game is. Before, the sound stayed off until you restarted the game.
- A game started with no sound device at all starts its sound once one is
  plugged in.

## 2026-10-24 · The game starts again after the computer sleeps
- If the computer sleeps while the game runs, the graphics card can lose
  everything the game showed on it (on Windows especially). The game used to
  stop with a crash when it next drew something. Now its window closes and
  the game starts again by itself, at its start screens, so you can log back
  in. The same happens if a graphics driver resets.
- If that happens again within half a minute of a start, the game stops
  instead of starting over and over, and offers a crash report the next time.

## 2026-10-24 · More ability bars; crash reports say when the game stopped
- Options > Extended has a new row, Ability Bars: 2 (the game's), 3 or 4. With
  3 or 4, the HUD has a third and a fourth ability bar, five abilities each,
  after the second bar when you switch bars (left and right, or a click on the
  bar's title). The spell book's Memorize offers their slots too: its list of
  slots shows ten at a time, and moving past the tenth shows the next ten.
- The server keeps which ability is on which slot, so the extra bars work
  only on a server that keeps them: eqoaclassic does, Sandstorm keeps only
  the game's two bars. If the server does not take an ability onto bar 3 or
  4, the chat says so in red.
- When the game offers a crash report as it starts, it now says how long ago
  the game stopped ("The game stopped 11 hours ago"), and says so when that
  was in an earlier version. Before, it said "last time" and showed the hour,
  in UTC, that the stopped game had been started.
- A crash report waits until you send it or delete it (Not now keeps it), so
  one offered right after you restart for an update can be from hours before.

## 2026-10-23 · The Who list as Sandstorm sorts it
- On Sandstorm, the Who list sorts the way Sandstorm's patch does. L1 and R1
  (or left and right) change the column, and arrows mark the column it is
  sorted by. The highlight stays at its place in the list instead of
  jumping to follow the player.

## 2026-10-23 · Sandstorm's player search, tall windows, a touch screen crash
- On Sandstorm, the Group menu's Search opens Sandstorm's own search window.
  It starts on levels around your own (45 to 60 at level 60), not every
  level. Change the race, the class or the levels, then pick Search.
- In the search window, pick a player to see where they are and their group.
  ✕ sends them a tell and □ invites them. L1 and R1, or a click on a column's
  heading, sort the list.
- In a window taller than it is wide (two game windows side by side, for
  example), the compass, gauges, buffs and quick menus stay at the top of the
  window, and the target, the F3 button and the chat stay at its bottom.
  Before, they drifted toward the middle.
- In such a window you see the world as wide as on a PS2, with more of it
  above and below, instead of zoomed in. The start menu shows your whole
  character again.
- With a touch screen, the game no longer stops when you come back into the
  world after logging out, being disconnected or restarting for an update.
- In the browser, the mouse look being refused for a moment (right after
  Escape let go of it) no longer pops up as a problem with the page.

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
