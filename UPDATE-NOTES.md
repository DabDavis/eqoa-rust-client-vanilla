# Update notes

What each dev build changed, for players, newest first: a section a build, added
in the same push as the changes it describes. The dev release's page shows them
(`release/notes.sh`, from `.github/workflows/dev.yml`); an edit to this file alone
is put on the page by `.github/workflows/notes.yml`.

## 2026-10-28 · You can run again after casting a spell
- Since about 00:55 UTC this morning, casting a spell left you walking, unable to run, until
  you logged in again: you walk while you cast, and the game thought the cast never ended. The
  clock change below already cured it; the cast's timing is now kept the way the PS2 keeps it,
  so no clock can bring it back.
- Dying now ends the slowed walk of a spell you were casting, as on the PS2.

## 2026-10-28 · The game's clock can no longer run out mid-session
- The game's network clock now starts from a small number each time you log in, so it can
  never reach the size that stopped the loading screen this morning, and it no longer runs
  out on November 21 (it would have frozen the loading screen, the alarm and some windows'
  timing for anyone playing at that moment, until they logged in again). Nothing changes in
  how the game plays.

## 2026-10-28 · Entering the world no longer closes the game
- Since about 00:55 UTC on October 28 the game closed itself as you entered the world, on
  every server, at the loading screen. Its turning gears were timed by a clock that counts
  milliseconds from the computer's date, and that count passed a size the gears' animation
  could not hold. The gears now keep a clock of their own, and no animation can step outside
  its frames any more, whatever its clock says.

## 2026-10-28 · Character select smooth at Render Scale x2 and x4, and HUD Nearest by default
- With Render Scale at x2 or x4 (Options, Extended, Video), character select and the other
  screens before the world are smooth again. The HUD's Sharp filter (Extended, UI, HUD), which
  draws the menus' art, did far more work for each pixel than it needed, and those screens
  are covered in that art from edge to edge: at the larger scales a browser could fall below
  30 frames a second there while Freeport, crowds and all, kept up. Sharp now does about a
  quarter of the work and looks exactly as before; the start menu's windows and the HUD in
  the world are lighter with it too.
- HUD is back to Nearest by default, the PS2's own way of drawing the menus' and the HUD's
  art, which costs the least to draw. Sharp is still on that row for anyone who wants every
  texel of that art drawn evenly at any window size (if you had chosen it, choose it again).

## 2026-10-27 · The touch controls laid out as a PS4 controller
- On a phone or tablet held sideways the on-screen controller is now laid out like a
  PS4 controller, with two sticks: the left one walks and the right one turns the camera.
  The d-pad sits above the left stick and the face buttons above the right one, L1/L2 and
  R1/R2 above those, Select and Start at the top in the middle, and L3 and R3 just above
  their sticks. Each stick starts where your thumb lands near it.
- Held upright, the same controller sits along the bottom of the screen.
- Every button now fits on the screen. In a phone's browser the bottom of the game used to
  sit under the browser's toolbar, which cut off the lowest buttons.
- All the buttons stay on every screen, Station's login and the windows over the game
  among them. Over a window they are drawn a little fainter, and a tap anywhere off them
  still goes to the window.

## 2026-10-27 · Full screen in the browser, and the page as an app
- In a browser the game now goes full screen as soon as you press Start: on an Android phone
  or tablet, and on a computer in Chrome, Edge or another Chromium-based browser. Escape is
  still the game's key there: tap it as always, and hold it to leave full screen. F11 still
  switches to a window and back.
- On Android, if you leave full screen (the back gesture, another app), your next touch on
  the game puts it back.
- With another window of the game open (multiboxing), Start leaves your windows as they are.
- The page can be installed as an app. In Chrome and Edge an "Install EQOA as an app" button
  shows under Start once you have used the page a little; on an iPhone or iPad choose Add to
  Home Screen in the browser's Share menu (iPhones let no web page go full screen, so this
  is the way there). Started from your Home Screen or your apps it is full screen with none
  of the browser round it, and it keeps the game files it unpacks from your disc between
  visits, so it starts quicker.

## 2026-10-27 · Running and turning at the PS2's own speed, at any frame rate
- Running, walking, turning and swimming now go exactly as fast as on a PS2, whatever your
  display's frame rate. Before, the higher the frame rate the faster you went: at 165 frames
  a second you ran about 5% faster and turned 6% faster than a PS2 player, and the speed
  also shifted a little with where you stood and which way you faced.
- It is as smooth as before: the game still draws every frame your display shows. Only the
  movement underneath keeps the PS2's own 30 steps a second, and your character is drawn
  on the way between them, on the controls you hold that moment.

## 2026-10-27 · Character select shows your race's set alone, and your volume from the start
- Character select and creation draw your race's own set and nothing past it, as the PS2 does:
  the other races' sets no longer show in the distance behind your character, and with them
  gone the screen draws much faster.
- Your Fx and Music Volume are used from the very first sound: the loading pictures, the
  agreement and the title no longer play at full volume before your settings take hold.
- In a browser on an iPhone or iPad, the disc image can be chosen: iOS greyed it out, as it
  knows no file type for a disc image. The page still checks that it is the right disc.

## 2026-10-27 · Updates with two windows open, and lighter fireballs
- An update now goes through while another window of the game is still open on the version
  before. On Windows a running program can't be replaced, and the copy the last update set
  aside was still running in that window, so every look for an update failed ("Access is
  denied") until every window was closed. If yours has said it could not update, close every
  window of the game once and start it again.
- Where the Lavastorm throws its fireballs (round Klick'Anon), each frame takes a little less
  of your computer: the client made 31 new buffers a frame for them, and now keeps its own.

## 2026-10-27 · A sharp HUD by default, and the world's textures smooth again
- The HUD's and the windows' art is drawn sharp now unless you choose otherwise, every pixel
  the same size at any window size, as the text already was. Options > Extended > UI > HUD
  puts back the game's own look (Nearest).
- Options > Extended > World > Textures is gone: the world's textures are drawn smooth, as
  the game draws them. Crisp did not suit them.

## 2026-10-27 · Render Scale, and a sharper HUD and textures if you want them
- Options > Extended > Video > Render Scale: x2 or x4 draws the whole picture two or four
  times larger and shrinks it to fit your window, which smooths jagged edges, leaves, fences
  and thin things. x1, the default, is as before. x4 needs a good graphics card, and on a big
  window it draws at the most your card allows (the page tells you when it does).
- Options > Extended > UI > HUD: Sharp draws the HUD's and the windows' art with every pixel
  the same size at any window size, the way Text: Sharp draws the letters. Nearest, the
  default, is the game's look.
- Options > Extended > World > Textures: Crisp draws the world's walls, props, characters
  and plants sharp, each pixel of their textures a flat square. The ground, the sky, water
  and glows stay smooth, so roads still blend into grass and snow. Smooth, the default, is
  the game's look.
- Text (Sharp or Smooth) is on the UI page now, beside HUD.

## 2026-10-27 · The map, the minimap and the extra bars are gone
- This client keeps to the game: the additions it never had are taken out. Gone are the map
  (M) with its waypoints, group waypoints, XP camps, coaches, routes and community markers, the
  minimap (N), the quest markers, the quest tracker and the quest guide behind them, the Stats
  Box (B), the power bars under your group, your target and its target, and the Target of
  Target bar.
- What a computer adds stays: widescreen, any frame rate, the mouse, the touch screen, the
  keyboard and the Controls page, controllers, several windows at once and the Extended options.
- F still targets whomever your target is targeting: it is the game's Assist Target.
- Your options, keys and buttons stay as you left them.

## 2026-10-27 · Multiboxing: a window in the back keeps its frame rate
- On Windows, the game now asks Windows not to slow it down while its window is not in
  front (Windows 11 slows programs in the back to save power), so the box you are not
  playing keeps its frame rate. On a Mac it asks macOS not to put it to sleep in the back
  (App Nap) or bunch up its timers.
- In a browser, the browser decides how fast a window in the back runs: on Windows 11,
  Task Manager's Efficiency mode (the green leaf) on the browser, or a power-saving power
  mode, can slow it.
- A problem report now says how fast each window drew while it was in the back.

## 2026-10-27 · The page says when your browser is not using the graphics card
- In the browser, if the game is being drawn without your graphics card (in software, as
  Windows' "Microsoft Basic Render Driver" does), the page now says so: on the start card, in
  a message at the top you can dismiss, and beside the frame rate in the corner while you
  play. The game runs slowly that way, whatever the settings. To fix it, turn on the
  browser's graphics acceleration (Chrome or Edge: Settings > System > "Use graphics
  acceleration when available", then relaunch the browser) and update your graphics card's
  driver.
- The program does the same check, and says so in its log, which a problem report carries.

## 2026-10-27 · Network Setup: finishing the address takes you back to the title
- On the title's Network Setup, finishing the server's address (Enter, or R2's End) now
  keeps it and goes straight back to the title, ready for Play. Before, the page stayed open
  and only △ (Escape) left it. An address that is not a server address is still refused
  with a message, and the page stays so you can fix it.
- The small edit box you get when typing on a keyboard: its End and Cancel can now be
  clicked or tapped before you are in the world too (Network Setup, Station's login, a new
  character's name), as they already could in the world.

## 2026-10-27 · Tab picks targets as on a PS2 in a wide window
- In a window wider than a TV's 4:3 (most monitors), Tab (R1 on a controller) now picks the
  next target in the same order a PS2 does: creatures in front of you and close come first,
  and ones off to the side later. Before, the wider the window, the less being off to the
  side counted, so a creature near the edge of the screen was often picked before one
  straight ahead.
- Creatures you can only see because the window is wide can still be picked.

## 2026-10-27 · A waypoint for your whole group
- On the map, hold □ (or Q) for a moment to put a waypoint for your whole group. Everyone in
  your group playing this client sees it on their map, minimap and compass, in orange, with
  your name under it on the map. A quick press of □ still puts your own magenta waypoint.
- On a keyboard G does it at once, or hold Shift and click a spot; on a touch screen hold your
  finger on the spot. G can be moved on the Controls page.
- It goes where □ would put yours: the middle of the map once you have moved the map, else on
  your target, else where you stand, to call the group to you.
- It reaches your group as a line in the group's chat, so it works on any server and a player
  on a PS2 reads it too: "Dairine tells the group: Waypoint in Tunaria at x 24611, z 15502 near
  Freeport". One every few seconds.
- The group's waypoint goes when you reach it, when the one who put it leaves the group or you
  do, or when you click it on the map. A newer one replaces it.

## 2026-10-27 · Waypoints and map suggestions on your target
- With someone targeted and in view (a merchant, a quest giver, a creature), □ (Q) on the map
  puts your waypoint right on them; press it again to take it away. A white ring shows your
  target on the map.
- Suggest (N, or L3 on a controller) takes your target's spot too, with their name already
  typed in for you to keep or change.
- What you choose on the map still comes first: once you move the map, □ goes in its middle,
  and Suggest takes the spot you point at or a waypoint you just put. With no target, both
  work as before.

## 2026-10-27 · No more creatures fighting you while lying dead
- A creature or a pet could come into view lying on the ground as if dead, then run after
  you and fight like that. It happened where many look alike, in a camp of one kind: when
  one came into sight just as a corpse of its kind went out of it, the game gave it the
  corpse's body. Each creature that comes into view now gets a body of its own, standing,
  as on a PS2.

## 2026-10-26 · Play on a phone or a tablet from the first screen
- On a phone or a tablet you can now play from the very first screen: tap your way through
  the licence, the title, Station's login, the server list and character select and
  creation. ✕ △ ○ □ stay on screen the whole way, and in the world the whole on-screen pad
  is there as before.
- In the browser, tapping a name or password box brings up your device's own keyboard. The
  game's on-screen keyboard still works too, by tapping its keys.
- The update notes' Back and the failed update's Restart can now be clicked or tapped.
- On a touch screen, the bug report's buttons now take a tap.

## 2026-10-26 · A Stats Box, and N for the minimap
- A Stats Box: your stats (Str to Cha, AC, max HP and power, the resists) in a small box
  under your effect icons. A buff that raises a stat shows it in blue, a debuff that lowers
  one in red. It is off unless you turn it on: Options > Extended > UI > Stats Box, or press
  B. Click or tap it to open your Summary.
- Press N to show or hide the minimap. Both keys can be moved, or given a controller
  button, on the Controls page.
- Options > Extended: Text (Sharp or Smooth) is on the Video page now.

## 2026-10-26 · Spell visuals on yourself and on a target at once
- On the Help match spell visuals page, "Lands on: the caster and a second human" plays a
  visual both ways at once: the human on the left casts it on itself while the one beside
  it casts it on a second human, so a buff, a debuff and a damage spell can be told apart
  without switching back and forth. The page remembers what you picked.
- The training dummy is gone from that preview: its small size shrank what landed on it.
  A harmful spell now lands on a second human.
- W and S step to the visual before and after in the list from anywhere on the page, the
  preview too, except while you type in a box.

## 2026-10-26 · A server's patch in the browser too
- In the browser the game now takes the server's patch, as the program does: on
  Sandstorm, its damage colours, robes and armour, menus, words and the rest, where the
  page played every server as the plain disc before. The patch comes from the server's
  own website (https), so a server whose site serves its patch files gives it; one that
  does not is played as before.
- The page keeps the patch between visits, so the next start fetches only what changed.

## 2026-10-26 · Community markers on the map
- The map shows places other players suggested and a GM accepted: a pin each, coloured by what it
  marks (a landmark, a camp, a quest's place, a merchant, a way to travel, a danger), its name
  beside it when you zoom in, and what it is when you point at it. X hides them with the other
  markers. The list comes from eqoaclassic.com whichever server you play on, and is kept for a
  start with no connection.
- Suggest one of your own: put your waypoint on the spot (a click on the map), or point at it, and
  press N (L3 on a controller, or the map's Suggest button). Type its name, then a note if you
  like; Enter sends it for a GM's approval, Escape sends nothing. The map says when it went.

## 2026-10-26 · Voice chat was only a test
- Voice chat was only a test, and it is out of the game and the page: the voice chat window (O)
  and the microphone badge beside the minimap, the /voice commands, Voice Chat, Voice Demo and
  Voice Volume in Options > Extended > Sound, and the push-to-talk keys (V and N) on the Controls
  page.
- The game asks nothing of your microphone any more. Your other options and keys are kept as
  they were.

## 2026-10-26 · A controller that turns up mid-game
- On Windows the game no longer stops when a controller turns up while it runs (Steam can make
  one as a window comes to the front, and two pads made it likelier): until Windows has finished
  connecting it, the game passes over what it sends, and reads it from then on.

## 2026-10-26 · New swings at once, Extended's menu
- A level reached while you play counts for your swings at once: at 20 and at 35, where a round
  gains hits, your character swings the new round straight away instead of after you log out and
  in again.
- Options > Extended's menu has room for all eight of its parts: Quests no longer sits on top
  of Select and Back.
- A problem or crash report says which version you are on and what the game found when it last
  looked for a new one, so a game that stops updating itself tells us why; a crash report also
  carries only that run's own details.

## 2026-10-26 · A voice chat window
- Press O (or type /voice in the chat) for the voice chat window: turn voice chat on or off and
  pick Push to Talk or Open Mic, see whether you are connected and how loud your microphone is,
  and which keys talk (V to your group, N to your room; change them on the Controls page).
- It lists your group and your room with a light by each person who is talking. Turn anyone up or
  down (0 to 200%) or mute them.
- Join a room from the lobby (a locked one asks for its password), leave the one you are in, or
  join a room by typing its name: that is also how you make one, with a password if you like, and
  unlisted if only people who know its name should find it.
- While voice chat is on, a small microphone sits left of the minimap: grey while you are quiet,
  green while you are heard. Click it to open the window.
- Works with the keyboard, a controller (✕ to choose, △ to close, L1 and R1 to jump between
  parts) and the mouse or touch screen.

## 2026-10-26 · Voice chat
- Talk to your group by voice: Options > Extended > Sound > Voice Chat, Push to Talk
  (hold V) or Open Mic. It is off until you turn it on, and nothing is asked of your
  microphone before then. Voice Volume sets how loud the voices are, up to twice.
- A lobby of voice rooms anyone can make: `/voice join <room>` joins one, or makes it;
  add a password to lock it (`/voice join <room> <password>`: the password never leaves
  your computer). `/voice unlisted <room>` makes one only those who know its name can
  find. `/voice rooms` lists them, `/voice who` says who is in yours, `/voice leave`
  leaves it. Hold N to talk to your room (Open Mic talks to both).
- A speaking mark lights on each groupmate's bar while they talk; the room's speakers
  show beside your own frame. Click a mark to mute someone, again to hear them.
- The voices go through the server, never from player to player, so nobody sees your
  address. The keys are on the Controls page, to move as you like.
- On a server without voice (Sandstorm), Voice Demo (same page) uses eqoaclassic's
  voice server instead: a demo, open to anyone who knows your group's names or your
  room's name.
- In the game itself (not the page) there is no echo cancelling yet: wear headphones
  for Open Mic.

## 2026-10-26 · Sound at 60 frames a second
- At Frame Rate 60 (or the display's) a busy moment lost sound commands: a sound could
  start at full volume however far away it was, or not start at all, and an ambient
  track that should have stopped could go on playing over the sounds after it. In a
  40-second test in the rain with a bard's songs, the last build lost 698 of them at
  60 frames a second; this one lost a handful, as the PS2's own 30 a second does.
- The few a very busy moment still has no room for (the PS2 loses those too) now go a
  frame later instead of never: a sound's volume, pan and pitch, and a track's stop.
- If the sound turns crunchy again, press F3 while it is: the report now says which
  kinds of sound commands were lost, and the log says when a voice played on with
  nothing left to stop it.

## 2026-10-26 · Quest markers and the tracker on Sandstorm too
- On a server that sends no quest markers or tracker of its own (Sandstorm), the game now
  makes its own from a guide built in: the quests of levels 1 to 20 of every home city's
  classes. A "!" at the NPC with your next quest, a "?" at whoever your quest wants next or
  where its creatures are, and the tracker under the minimap.
- It reads your quest journal to know where you are, so it can be wrong: a quest it does not
  know shows as the journal says it, and everything it made says "(guide)".
- Items are counted from your bags and what you wear ("Slashed Pawn Belt: 1/2"); kills only as
  the game saw you or your pet make them, so they say "(seen)".
- It remembers the quests it saw you finish on this computer. On our own server nothing changes:
  the server's markers and tracker are used.
- A quest name or goal too long for the tracker now goes onto a second line.

## 2026-10-26 · The map shows where your pointer is
- Under the map's title, the place under the mouse pointer, as the game counts
  it: "x 24611, z 15502" (on a controller, the place under the middle cross once
  you pan the map off yourself). Thanks for the idea.

## 2026-10-26 · Two windows, one PlayStation controller
- With two windows of the game on one PlayStation controller (Windows), a press
  made while a window is busy coming to the front is no longer lost: the game
  now keeps every report the controller sends until it can read them, as it
  already does for the keyboard. Before, the second window could ignore the
  controller for up to half a second as it came to the front.
- A problem report from one window now also carries the other windows' last
  lines on coming to the front and on their controllers, so a delay in one
  window shows up whichever window you press F3 in.

## 2026-10-26 · A quest tracker
- Under the minimap, the quests you are on and what each one wants next, counted as you go:
  "Bat Eye: 1/3", "Orc Pawn: 2/3", "Speak with Kellina". A goal you have all of turns grey.
  On a server that sends it (ours does; Sandstorm sends none).
- It has no box behind it, so the world shows through; when there is not room for all your
  quests it shows the first ones and how many more there are.
- Click it to open your quest journal. Turn it off under Options > Extended > Quests >
  Quest Tracker.

## 2026-10-26 · Quest markers on the map
- The map shows where your quests are, on a server that sends them (ours does;
  Sandstorm sends none): a gold "!" at each NPC with a quest you can take now,
  for your class, race and level, and a "?" where a quest you are on needs
  you: the NPC to see next, or the nearest places of the creatures to hunt.
- Point at a mark (or tap it, or pan the map's cross onto it with a
  controller) to see its quests: their names and levels, how the offer starts
  or what the step asks, and the rewards or what you still need. Click it, or
  tap it again, to put your waypoint on it.
- The marks sit over the XP camps and coaches, and are on the minimap too, on
  its edge while farther than it shows (point at one for its quests). J on the
  map (R3 on a pad, or the map's Quests button) hides or shows them, and so
  does Options > Extended > Quests > Quest Markers.

## 2026-10-26 · A minimap
- A minimap in the top right, under the name plate of the menus beside it: the map's own
  picture of the land round you, north up, with you as the arrow in the middle, your group in
  their colours, the creatures and players near you as dots in their names' colours, your
  target ringed and your waypoint (on its edge while it is farther than it shows).
- The map's XP camps and coaches are on it too, as the map marks them; point at one with
  the mouse for its card. Map Markers (X on the map) hides them here as well.
- Click or tap it to open the map; the mouse wheel over it zooms it in and out. It goes
  away with the rest of the HUD under Hide UI (H) and while a window is open.
- Turn it off under Options > Extended > UI > Minimap. Loading Screens moved from UI to the
  Video page to make room.

## 2026-10-26 · XP camps, coaches, routes and towns on the map
- The map (M) marks the popular XP camps with a red XP badge and every coach with the
  horseshoe the stables wear (the one on Freeport's stable sign, read from your own disc).
  Point at one, with the mouse, a tap, or the pad's middle cross once the map is panned,
  for its card: a camp's levels, the guide's note, the rare creatures near it and notable
  drops; a coach's town, its coachman and the towns it runs to.
- Click a marker, or tap it a second time, to put your waypoint on it.
- The coach routes are dashed lines between the towns they join; the coach you point at
  has its own drawn bright.
- The towns' names are on the map too, blue for good, red for evil, cream for neutral,
  the bigger town's first where they crowd, smaller as you zoom out.
- X (○ on a pad) hides or shows the markers and routes, and so does Options > Extended >
  World > Map Markers; T (Select) the town names. The camps, routes and towns are from
  EQOA-Data's map guide.

## 2026-10-26 · Lightning you can see
- Lightning spells shoot their bolts in the world now: a wizard's Static Bolt, Shocking
  Strike, Lightning Blast, Thunderbolt and the rest, and every other spell whose visual
  has lightning, yours and other players' and creatures', whoever they hit. The bolts
  were worked out all along but drawn where nothing is; the spell preview always showed
  them.
- Weapons leave their swing trails in the world now too, hidden the same way until now.

## 2026-10-26 · Target of Target above the target
- Target of Target (Options > Extended > UI) now sits just above the target's bar, with the
  health and power of whoever your target is targeting under it, as a group member's are:
  beside the target it went behind the chat box once the chat held a few lines.

## 2026-10-26 · One visual for several spells
- On the Help match spell visuals page, tick the spells that share a visual (a spell
  line's ranks often do), pick the visual once, and send: it goes for every ticked spell.
  Tick all ticks every spell your filters show (search "pact" and tick them all at
  once); Clear ticks starts over; with nothing ticked the page works as before.

## 2026-10-26 · An icon of its own
- The game has an icon of its own now: a disc, on the program and its window. Once it
  has read your disc image it shows the game's own memory card icon instead, the head a
  PlayStation 2 showed for the EQOA save (drawn from your disc and kept for the next
  start). The page has the disc as its icon too.

## 2026-10-26 · Masteries, epics and master-class spells to match
- The Help match spell visuals page now lists the class masteries, the level 60 epics and
  the master classes' spells too, each tagged as such; search "epic", "mastery" or a master
  class's name (Lich, Sorcerer...) to find them.

## 2026-10-26 · An even pulse at 30 frames a second, and the spell list shrinks as visuals are set
- With Frame Rate set to 30, the pointer beside a list's selection (the who list, the
  menus) pulses evenly again, and the HUD no longer skips a beat every few seconds: the
  cap now runs at exactly the game's own 30 frames a second.
- On the Help match spell visuals page, a spell leaves the list once GMs have set its
  visual, so what is left is what still needs a match.

## 2026-10-26 · Help match spell visuals
- A new page, eqoaclassic.com/spellfx (linked under the game's page as "Help match spell
  visuals"): pick a spell (filter by class), try the visuals off your own disc in a
  preview, and send the one that spell really had, with your name and a note if you like.
  No sign-in; GMs compare everyone's picks and set the right one. Your disc image stays
  on your computer.

## 2026-10-26 · The browser remembers your disc images properly
- In a browser, "Remember my disc images" now also keeps a disc you chose before
  ticking it: ticking it opens the picker so you choose the disc once more, and that one
  is kept. On your next visit, press Start and allow the browser to read it when it asks,
  and the game starts with your remembered disc.

## 2026-10-26 · A waypoint of your own on the map
- The map has a waypoint of your own: click or tap a spot on the map (M) to put a
  magenta marker there, and click it again to take it away. With a controller, pan the
  map and press □ to put it at the middle (Q on the keyboard does the same).
- The compass shows it as a magenta dot on its rim pointing the way to go, however far
  it is, and it goes away when you get there. It is kept for each world, and nobody
  else sees it.

## 2026-10-26 · Sound that stays clean, the training dummies' string, and remembered discs
- Sound no longer turns crunchy after a busy fight: with many spells sounding at once, a
  sound that was loading at that moment could be stored with part of it missing and play
  garbled for the rest of the session (a PS2 does the same), and a sound's stop could be
  lost and leave it looping. If a crunch ever comes back, press F3 while it lasts: the
  report now says whether any sound is stored wrong and whether your computer's sound
  output fell behind.
- The training dummies, and a few other tree-shaped creatures, no longer trail a thin
  string from their hand down to the ground: this client drew the last points of their
  hands at their feet.
- In a browser: tick "Remember my disc images in this browser" (Chrome, Edge and other
  Chromium-based browsers) and pick your discs once; next time they're put back for you,
  or offered on a button when the browser wants to ask first. The images stay on your
  computer.

## 2026-10-25 · Mana bars, rumble for PlayStation controllers, and multiboxing fixes
- Mana bars: on a server that sends them (eqoaclassic does), each group member,
  your target and your target's target show a power bar under their health.
  Options > Extended > UI > Mana Bars turns them off. Sandstorm doesn't send
  anyone's power, so you won't see them there.
- Target of Target now shows its health and power beside its name.
- PS4 and PS5 controllers rumble on Windows now, as on a PlayStation 2: when you
  cast a spell and when one hits you (melee never rumbled in EQOA). Options >
  Vibration turns it off.
- On a Mac, PS4 and PS5 controllers plugged in by USB rumble too. Over Bluetooth
  they don't yet.
- On Windows a PS4 controller's light no longer goes dark while the game runs.
- A short rumble is no longer lost when the game is slow for a moment, like when
  the world loads.
- Run `eqoa rumble` from a terminal to check that a controller rumbles.
- Multiboxing: a window on Controller Focus Auto no longer takes the controller
  while another window on Window is in front, so one press reaches one window.
  With Capture Controller on, a window on a Pad does the same.
- Problem reports say more when a window comes to the front: how long it had
  stopped drawing, any controller press it held back and why, and a PlayStation
  controller that stopped answering.
- In a browser, a problem report now says how the game's connection to the
  server stands, so a connection that went quiet shows in the report.

## 2026-10-25 · Clean sound in long fights, loading screens on long trips, and Klik'Anon
- Sounds no longer turn crunchy or garbled after a lot of fighting and spells. A
  sound let go while it was still loading could leave the next one made in its place
  playing the wrong noise until you restarted the game, on a PlayStation 2 too.
- A creature's own looping sound now stops when it leaves or changes.
- Return Home, coaches and other long trips now show the game's loading screen for
  about a second instead of jumping you to the new place in a blink. Options >
  Extended > UI > Loading Screens: Longer, the default, or Instant to have it gone
  as soon as the place is in.
- Klik'Anon is spelled as players know it, in the place names and on the gnomes'
  page.
- If the game closes itself while starting on your graphics card, the next start
  tries the card's other way of drawing (DirectX 12 on Windows, OpenGL on Linux) and
  keeps it once it works. Each step of the start is in the log, so a report says
  where it stopped.
- In a browser, the game keeps playing while its tab is hidden behind another one,
  so you stay logged in. Before, it stopped the moment the tab was hidden and the
  server let you go after about 8 minutes. A tab the browser puts to sleep (a laptop
  asleep) still stops.
- Problem reports now say how many sounds the game holds and how much of its sound
  memory they use.

## 2026-10-25 · Turning with the mouse keeps you running
- Turning around with the mouse while you run now keeps you running the new way at
  once. Before, a quick half turn made your character skid to a stop, stand, walk
  and only then run again. Sticks and keys turn as before.

## 2026-10-25 · Back into Sanctuary at login, the camera at a wall, and quick presses
- Logging in where you logged out in another world takes you back there: a vampire
  who logged out in Sanctuary came back to an empty hillside in Tunaria. The game
  now loads the world the server names, as a PlayStation 2 does.
- When a wall, a tree or a hill comes between you and the camera, the camera now
  comes in only as far as it has to, stopping just in front of it instead of right
  behind your head, and goes back out to the distance you chose as the way clears,
  following it out as you move. Within a few steps of you it still does what the
  game does.
- A controller button tapped quickly, or pressed just as a game window comes to the
  front, is no longer missed: the game read the controller once a frame and lost a
  press that came and went in between. Xbox controllers on Windows are still read
  once a frame.
- Options > Extended > Characters: NPC/PC View Distance is now NPC/PC Distance, and
  its number no longer runs into its name.
- Problem reports now say how each player near you is moving (what they are doing,
  how fast, and how long since the server last placed them), and, when a game
  window came to the front, which keys were still held and when your first press
  arrived.

## 2026-10-25 · Grass, shrubs and stones on the ground, and a World options page
- The ground now grows the plants the game puts on it: tufts of grass, shrubs, cacti
  and stones round you wherever the game has them, such as the dunes round Freeport,
  fading out a few steps away, as on a PlayStation 2. They had never been drawn.
- Options > Extended has a new World page: Fog, Mipmaps, Flora, Environment and Art.
  Flora turns the plants off. Environment turns off the ground fog, dust, falling
  leaves, light shafts and flocks of birds. Both are on by default. Fog and Mipmaps
  moved here from Video, and Art from Characters.

## 2026-10-25 · Textures in the distance as the PlayStation 2 draws them
- Ground, walls, trees and everything else far off now use their textures' smaller
  versions, blended by distance, as the PlayStation 2 draws them: the shimmering
  ripples on distant sand and the crawling edges of far-off palm leaves are gone.
  Up close nothing changes.
- Options > Extended > Video > Mipmaps turns this off, if you like the sharper look;
  it is on by default. Art (Frontiers' or the 2002 disc's) moved to the Characters
  page to make room.

## 2026-10-25 · A window holds you in place, and the left mouse button steers on the move
- When an NPC talks to you, or any window is up (a question, a corpse, a merchant,
  the bank, the trainer, the spell book...), you stand still and the camera stays
  where it is until you close it, as on a PlayStation 2. With autorun on, the run
  waits and goes on once the window is closed. The chat line, a tell, the quick
  chat and the chat box's history still let you move.
- While you move (the keys, a stick or autorun), holding the left mouse button now
  steers you as the right button does: you turn to where the camera looks and run
  that way. Standing still, it still looks round you without turning you.

## 2026-10-25 · Vampires can go to Sanctuary on Sandstorm
- On Sandstorm, a vampire's Sanctuary no longer answers "You can't go to Sanctuary
  (memcard patch 0 outdated)". The game now tells Sandstorm it plays with
  Sandstorm's own patch, as a PlayStation 2 with that patch does.
- What it sends with that is what the patch sends: no network adapter's address,
  an id the game makes once and keeps in that server's folder, and, with each
  ability you use, when you pressed its button and how long you held it the time
  before.
- Other servers are unchanged.

## 2026-10-25 · Names over heads in a crowd
- Options > Extended > Characters > Overlapping Names: Show puts a name over every
  character within Name Distance, even in a crowd, the nearest on top. Hide, the
  default, is the game's own rule: in a crowd, a name that would overlap a nearer
  one is not drawn.
- Names over heads no longer flicker against a head, hat or helm. Where a name sits
  is the game's own: with the 2002 art (F9) that is right on top of the head, as on
  the 2002 disc.

## 2026-10-25 · Pick where the sound plays, and full screen from the options
- In the downloaded game, Options > Extended > Sound > Output Device: play the
  game's sound on the speakers or headphones you pick. Left on Default, it now
  follows your computer's default output when you change it there; before, it
  stayed on the device that was the default when the game started. The Sound page
  shows where the sound is playing.
- In the downloaded game, Options > Extended > Video > Display: switch between a
  window and full screen. Alt+Enter does it too, as F11 already did (in a browser,
  F11 is the browser's own full screen).
- Text (Sharp or Smooth) has moved to Options > Extended > UI.

## 2026-10-25 · Damage numbers, and targets with no body
- Damage and healing numbers stay over their creature while it runs, instead of
  shaking at high frame rates.
- Creatures with no body of their own, like the Ice Statue in Geyvax Cave, can be
  targeted with R1 (or the mouse), as on a PS2, and show their target rings.

## 2026-10-25 · Coaches and ports no longer disconnect you on Sandstorm
- A coach or a harbor to another world, a port, or Return Home no longer
  disconnects you on Sandstorm with "This character has been modified by a
  customer service representative". While the new place loaded, the game went on
  telling the server it stood where it had left; it now tells the server it has
  arrived only once it stands at the new place, as a PS2 does.

## 2026-10-25 · Options > Extended sorted into pages
- Options > Extended now opens a short list: Video, UI, Controller, Camera,
  Characters and Sound. Each opens a page with just its own settings, so none of
  them scrolls any more.
- Triangle (or Escape) goes back one step: from a page to the list, from the list
  to the Options menu. Every setting keeps its value and works as before.

## 2026-10-25 · Level of Detail at any zoom, and names when zoomed out
- Level of Detail Off now keeps every character at full detail however far you zoom
  the camera out. Zoomed out, your own character's fingers, toes and face used to
  stop moving.
- With Level of Detail On, characters move as the PS2 shows them from its own camera,
  NPCs and players alike, however far you zoom out.
- Names over heads stay when you zoom out. Pulled back past the PS2's camera
  distance, the names around you used to disappear, and at the farthest zoom none
  showed at all.
- Names get smaller as you zoom out, down to half size at the farthest, so they no
  longer cover the characters; zoomed in they are the size they always were.
- Options > Extended > Characters > Name Distance: how far away names show, from the
  game's 22 up to 200.

## 2026-10-25 · Clearer text, and a Text setting
- Text is drawn evenly at every window size, names over heads too: before, at most
  sizes some letters came out a pixel thicker than others (the game's font is made
  for the PS2's small screen), which looked rough or blurry, worst in smaller
  windows.
- Options > Extended > Video > Text: Sharp (crisp, the default) or Smooth (softened,
  the way the PS2's text looked on a TV).

## 2026-10-25 · Hide the UI, and Target of Target
- Press H to hide everything drawn over the world: the HUD, the chat, the names over
  heads, the damage numbers and your target's rings. Press H again to bring it back;
  any window that opens (the start menu, an NPC's words, a message, the map...)
  brings it back too. Options > Controls moves it to another key or puts it on a
  controller button.
- Options > Extended > UI > Target of Target (off unless you turn it on): beside your
  target's bar a second bar shows who your target is targeting. On eqoaclassic it
  shows for players and monsters alike; Sandstorm sends it for players only, so
  there it shows when you target a player (your tank, say), not a monster.
- Press F to target your target's target: the game's own Assist Target, the server
  telling the game whom to target. Clicking the Target of Target bar does the same.
  Options > Controls can move it.

## 2026-10-25 · Shouts stay off, and Update Notices
- Ignore Shouts (in the quick chat) now stays on until you pick Restore Shouts, even
  after you log out and back in: the game remembers it, tells the server again each
  time you enter the world, and hides any shouts that still come through. Your own
  shouts still show when you shout.
- Options > Extended > UI > Update Notices: turn it off and the game never stops you
  mid-play to ask about restarting for a new version; the new version simply starts
  the next time you start the game. In a browser it hides the Reload notice the same
  way.

## 2026-10-25 · Every hit of a double or quad attack is heard
- Each hit of a double or quad attack makes its sound, yours and everyone's, as on a
  PS2: before, only the first hit of each round was heard.

## 2026-10-25 · PlayStation controllers on Bluetooth, two windows' controllers, the Steam Deck
- On Windows a DualSense (PS5 controller) on Bluetooth works again: the d-pad no
  longer holds up by itself, triangle is triangle (it opened the quick chat, as L2
  does) and the shoulder buttons do what they say. If anything is still wrong with
  yours, press F3: a report now shows exactly what the controller sends.
- With two or more game windows open, entering the world no longer counts one window
  too many for a moment, which could hand a controller to the wrong window.
- A game window you close lets go of its controller at once, so the window you open
  next gets it, even when the closed one is slow to finish shutting down.
- Steam Deck, Game Mode: when the game cannot find your disc image it now looks where
  a Deck keeps games (your Downloads, Desktop, Documents, Games or ROMs folder,
  EmuDeck's Emulation/roms/ps2, and the same on the SD card) and otherwise asks in a
  window Game Mode can show. Before, it asked in one Game Mode does not show, so the
  game could look stuck. A disc image on a USB drive is not there in Game Mode: keep it on
  the Deck or its SD card.
- The game can no longer freeze when whatever started it never reads what it
  writes out, as some launchers do.

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
