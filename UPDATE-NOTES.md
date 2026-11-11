# Update notes

What each dev build changed, for players, newest first: a section a build, added
in the same push as the changes it describes. The dev release's page shows them
(`release/notes.sh`, from `.github/workflows/dev.yml`); an edit to this file alone
is put on the page by `.github/workflows/notes.yml`.



## 2026-11-11 · Hide the look of a worn item
- In the start menu's Inventory, Equipment tab, a worn helm, chest piece, bracers, gloves, leggings, boots, robe,
  weapon, shield, held item or cape now has **Hide Look** in its menu, right under Unequip. The item stays worn
  with all its stats and effects, but nothing of it is drawn on your character, for you and for everyone around
  you (a hidden weapon still swings as that weapon does, with nothing in the hand). Choose **Show Look** on it to
  see it again. The choice is kept for the slot, so a new helm you put on where the old one was hidden stays
  hidden until you show it, and it is the same at character select and at your next login.
- It needs the server's own update; on any other server the menu is as it was.

## 2026-11-11 · Whose mercenary is that?
- A mercenary now has a second line under its name over its head saying whose it is, such as
  "<Lonewulf's Mercenary>", so you can tell your mercenary from another player's. A mercenary's pet has no such
  line. It needs the server's own update (the Heirloom Test Server); on any other server the names are as they were.

## 2026-11-11 · Crash reports go to the server you play on
- If the game ever stops with an error, it now keeps a short **crash report** (what went wrong and where in the
  game's code, the build, your system and graphics card: never your account, your address or your files). The next
  time you log in to a server that takes them, it goes to that server's GMs as a ticket marked **Bug Report /
  Crash**, and you are told its number in chat. You don't have to write anything, and no answer is needed: it
  helps the developers find and fix the crash. Nothing is sent anywhere else, and a server that doesn't take
  them is sent nothing.

## 2026-11-11 · A Wisp keeps its cape while invisible, the Wisp's Raiment
- An invisible Wisp keeps its cape: a Wisp already shows as what hangs on an invisible body, so invisibility now leaves
  it as it was, the wisp in its helm and its cape, for you and for everyone who sees you. (Anyone else's cape still
  vanishes with them.) It needs the server's own update.
- On the Heirloom server, Trickster Flame now also teaches **Wisp's Raiment**: a Wisp keeps its own look, the wisp
  burning in its helm, and shows all its gear, armour, robe, weapons and cape, for an hour. Remove Illusion, a glamour
  or clicking it off ends it. A Wisp who already has Trickster Flame finds it in its spell book.

## 2026-11-11 · Wisps walk without a sound
- A Wisp makes no footsteps: its walking, running, turning, falling, landing and swimming are silent, yours and every
  other Wisp you meet. Its swings and spells sound as ever. In a mortal form (a glamour from its racial Trickster Flame)
  it is heard as the body it shows.
- On a server that hides a Wisp's gear (the Heirloom server), a Wisp in its own form shows only the wisp and its cape;
  in a glamour it shows its whole body in its armour and weapons.

## 2026-11-11 · Look at any world on the map, the minimap indoors, a profanity filter switch
- The map has a **Places** button (P on a keyboard, Start on a controller): a list of the worlds (Tunaria, the Rathe
  Mountains, Odus, Lavastorm, the Plane of Sky and the server's own worlds) and, under the world shown, its towns.
  Pick one to look at it from wherever you are: in Tunaria and want to see Oggok? Pick the Rathe Mountains, then
  Oggok. A world you have never been to is drawn from your disc the first time (it takes a little while, and is kept
  for next time). "Where I am", Center or closing the map takes you back to your own spot.
- The **minimap now maps indoors**: inside a building, in a cave or in a dungeon it shows the floors around you
  instead of the roof or the hillside above you, and zooms in to a room's size (the mouse wheel over it still
  zooms). Step back outside and it shows the usual map again.
- **Options > Interface > Profanity Filter**: On (as always) or Off. Off, what other players say reaches you as
  they said it. It needs the server's own update; on any other server nothing changes.

## 2026-11-11 · Dismiss your mercenary from the menu, mercenaries' names, the group search's register
- On a server with mercenaries (the Heirloom Test Server), Community > Group now has Dismiss Merc while your
  mercenary is out: it sends it away as /merc dismiss does.
- A mercenary's name over its head (and its dot on the minimap) is now the pets' blue with a slight red tint, so you
  can tell a mercenary from a pet at a glance.
- The group search (Community > Group > Search) shows at its foot whether you are registered as looking for a group;
  select it to register or unregister. Registered players now show up in every player's search, not only in a group
  leader's, so anyone can find you and invite you or send you a tell.
- In the search's list, Options > Tell now starts the tell in your chat line ("/tell <name> "), ready for you to type.
- A new setting at the search's bottom right, "Allow anyone in my group to invite" (Yes or No, kept for each
  character): when you lead a regular group and set it to No, only you can invite into it; members of your group
  see it at once. Yes (the default) lets every member invite, as before. Raid groups are not affected.
- All of this needs the server's own update too; on any other server the menu, the names and the window are as they
  were.

## 2026-11-10 · No more long wait after Enter Game World
- In the browser, your character no longer sits at the spinning figure for 10 to 30 seconds after Enter Game World
  on a server with worlds of its own (the Homelands). The page used to hold every character until it had built or
  checked those worlds, on every visit. Now only a character who is in one of those worlds waits for them, a few
  seconds; every other goes straight in, and the worlds finish building while you play (the notice at the top of
  the page still says how far the build has come).
- The program now waits those few seconds too for a character who is in a homeland. Before, a new Aviak or Kerran
  played right away from the program started in its mainland city instead of its homeland, and stayed there.
- Going in before the worlds are built no longer leaves you without them until you log in again: once they are
  built you can be sent into them (the Canyon of Banners' matches, for one). Before, the program often told you
  "Your game cannot see the arena" for the rest of that visit.
- All of this needs the server's own update too; on a server without it everything is as it was.

## 2026-11-10 · A server's own races load faster at character select
- On a server with races of its own (hill giants, fungusmen, kerrans, wisps...), character select and character
  creation used to stutter while those races' bodies were read off your disc: the whole 148 MB creature file was
  read for a handful of bodies, twice, and in the browser it was held in the page's memory. Now only the bodies
  themselves are read (a few MB), in the background, and a character already on screen is no longer redrawn from
  scratch when more bodies arrive. Everything looks the same; the bodies show sooner, the pauses from reading
  them are gone, and the browser page uses about 240 MB less memory there. (A short pause the first time a
  creature's body is dressed in armour is still there.)

## 2026-11-10 · The PS2 loader linked from the browser page
- Playing on PCSX2 or a real PS2? The browser page's start screen now links eqoaclassic's loader (EQOABOOT.ELF and how
  to use it: your own unmodified disc, no patched ISO). On a server that offers no loader nothing new is shown.

## 2026-11-10 · Item links in chat
- On a server that offers them, you can link your items in chat. In your inventory, choose an item and pick
  Link Item, or Shift+click it (in the bank too): the chat line opens with the item's name in it in purple. Type
  around it as you like and send; the name stays one piece (Backspace takes it whole). Up to four in a line. If
  you back out of the line with a link in it, it waits for you: link another item, or open chat again, and it is
  still there until you send it or clear it.
- Links in the chat box are purple. Click one to open a small window at the top middle of the screen with the
  item's icon, name and stats, as the player had it when they linked it (its condition and charges too). Two can
  be open at once; a third replaces the oldest. Close one with its x, or the newest with Escape or Triangle.
- On a controller, hold R2 (the chat history) and press left to step up through the links in the chat, right to
  step back down, and X to open the one lit. Let go of R2 and you are back in the game; next time you hold it you
  start at the bottom again. With a keyboard, hold C and use the arrow keys and Enter.
- While you hold R2 (or C), the mouse wheel scrolls the chat history wherever the pointer is.
- Extended > Chat > Item Links turns it all off (it is on by default): then links show as plain item names, as
  they do for PlayStation 2 players and older clients.

## 2026-11-10 · A monster's name and level on Q
- On a server that turns it on, pressing Q (□ on a controller) on a monster that has nothing to say shows its name
  and level, and nothing else, in the same small window a player's details open in, for about 3 seconds (or for as
  long as you hold Q). Pressing Q again, another target or opening a menu closes it sooner.
- It works from any distance, as a player's details do. A monster you can talk to still talks, and Q on everything
  else is as it was.

## 2026-11-10 · Community > Group > Raid is there
- On a server with raids, Community > Group now shows Raid, right after Leave, where you see it without
  scrolling: the start menu dropped it each time it opened. Roll and Follow are now below it (scroll down).

## 2026-11-10 · Raids: the leader's crown, loot rolls, a master looter and new commands
- In a raid the raid list shows who leads it with a gold crown by their name, its helper with a blue cross and
  its main assist with a red diamond.
- When your raid's leader sets the loot rules to Need Before Greed, each item a raid boss drops, each no-trade
  item and each item with stats (never a stack or vendor trash) pops up in a small window at the right of the
  screen, under the minimap: Need, Greed or Pass, with 120 seconds to choose. Need is greyed out if you cannot wear or use the item.
  It never stops you moving: click a choice, or on a controller hold R2 and press right to reach it, then left
  and right to pick and X to choose (up and down for another item); let go of R2 to go back. With a keyboard,
  hold C and use the arrow keys and Enter. Your roll (0 to 100) is made the moment you choose, and every roll and
  the winner show in raid chat: any Need beats any Greed. The winner's item goes straight into their pack; with a
  full pack it waits on the corpse for them alone for 5 minutes (open the corpse to take it).
- The raid's leader can set the loot rules in Community > Group > Raid > Loot Rules: Free for All, Need Before
  Greed or Master Looter. They cannot be changed while the raid is in combat, and the raid cannot be disbanded
  then either.
- With Master Looter, a raid kill that drops such an item is the leader's alone to open for 5 minutes (anyone else
  is told whose it is; then it is anyone's). Choosing an item gives the leader a menu: Take, Inspect, Assign (pick a
  raid member from a list, then confirm) and Abandon (after a confirmation, anyone may loot anything on the corpse).
- A member kicked from the raid during a fight still gets its rolls (or can be assigned an item) for what the raid
  was fighting, so no one can be kicked out of their loot just before a boss dies.
- Community > Group > Raid also has Invite (your target), Raid Flag (everyone near), Kick, Make Leader, Make
  Helper, Make Main Assist, Assist Main Assist and Leave Raid, each shown to whoever may use it.
- New commands, typed in chat (on a PS2 put /t in front: /t raidinvite Bob): /raidinvite [name] (your target
  without a name), /raidaoe (or /raid), /raidleave, /rsay to talk to the raid, /raidloot, /raidleader, /raidhelper,
  /raidkick, /raidassist (set the main assist, or with no name target what your main assist is fighting: put it
  in a quick chat line to fire it in a few presses), /invite <name> to invite anyone to your group, and /target <name> (or /tar) to target the nearest player
  or creature whose name starts that way, within 100. /help lists them all.

## 2026-11-10 · Firefox and WebGL2: no more long crashes
- In a browser drawing with WebGL2 (Firefox on Linux, Mac or a phone, older Safari), when the browser's
  graphics stopped (its graphics process restarting, the driver resetting) the game went black and ran
  on unseen for about a minute, using more and more memory, until it stopped with an unreadable error.
  Now it stops at once and says what happened, with Reload.
- With Light Shadows on in WebGL2, the shadows are drawn the way they were before today's soft edges,
  which ask less of the graphics card. A browser drawing with WebGPU (Chrome, Edge) and the game you
  download keep the soft edges.
- If a browser's graphics refuse Dynamic Lighting, Sun Shadows or Light Shadows, that option is left off
  with a notice saying so, instead of the game stopping every time you enter the world.

## 2026-11-10 · Third-party PS3 controllers: the d-pad works
- A controller whose d-pad is a "hat", as on most third-party PS3 controllers and many other PC controllers,
  now works wherever the d-pad does (the menus, the quick menus, every window): in Firefox on Windows, in
  Chrome, Edge and Firefox on Linux and Chromebooks, in Firefox on a Mac, and in the downloaded game however
  the system hands the d-pad over. A diagonal presses both of its directions. Controllers that worked before
  work as they did.
- In the downloaded game on Linux, a controller the game knows no layout for has all its buttons now (before,
  only its d-pad and left stick worked), numbered as most PlayStation-style controllers number them: □ ✕ ○ △,
  L1, R1, L2, R2, Select, Start, L3, R3; its right stick turns the camera. On Android the same for a controller
  Android has no layout for. A button in the wrong place can be moved on the Controls page.
- In the downloaded game on Windows, such a controller (two sticks, no analog triggers) no longer seems to
  hold L2 while its right stick rests: L2 and R2 are its own L2 and R2 buttons, and its right stick moves the
  camera up and down again.
- In Firefox on Windows such a controller's right stick moves the camera up and down again too.
- In Chrome on Linux, a third-party PS3 controller's right stick pushed left while pressing up on the d-pad
  no longer mixes up its buttons (L2 and R2 held, the d-pad gone) for the rest of the session.
- A bug report (F3, or /bug) now tells the GMs which controllers you have and how the game reads each one, so
  one that misbehaves can be put right.
- Safari does not show a page such a controller's d-pad at all: play in another browser, or the downloaded
  game.

## 2026-11-10 · Wisps cast no shadow, gargoyles swim, the goblin chieftain's cape
- A Wisp no longer casts a shadow: there is no round shadow under it, and with Sun Shadows or Light Shadows on,
  nothing of it casts one, not its armour, its weapons, its helm, its cape or the wisp itself. Its shadow was a
  human's, and showed what stands inside the armour. Everyone else's shadows are exactly as they were.
- A Gargoyle swims in water now. Its body has no swimming of its own, so in the water it hung upright with its
  wings spread, as if flying; it now swims the way a human does, lying along the water and stroking, moving or
  still. The Jal'Raeth, who sat cross-legged in the water, swim the same way. A pixie or a fairy still hovers in
  the water: everything they do is flying. Other players with this client see them swim; a PlayStation 2 still
  shows them as before.
- On a server with jumps, a character in one of a race's other bodies (a shadow or fire gargoyle, a goblin king,
  any body the race's appearance choices offer) now falls and lands as its race's own body does. Through a
  jump or a fall off a ledge it stood in the air as if on the ground.
- A goblin in the chieftain's body wears its cape the way the other goblins do. The chieftain's spiked shoulder
  harness made its cape more than twice a goblin's width and depth, so it stood out from its back like a board and
  hung wider than its arms; it is now sized to the goblin's shoulders and hangs down its back over the harness. The
  goblin mystic's cape is a little narrower too (its spiked shoulder pads did the same, a little); every other
  goblin's cape is exactly as before.

## 2026-11-10 · A DPS meter
- A damage meter for your fights, alone, in a group or in a raid: who did how much damage and how fast. Turn it
  on in Options > Extended > Combat > DPS Meter (it starts off). It sits on the right of the screen above the
  chat box, under the quest tracker.
- Each row is a player: their place, name, damage, damage per second and share of the whole, over a bar in
  their class's colour. Alone you see yourself and, under your row, your pet. In a group or a raid group you see
  every member; in a big raid the top rows show, your own row always among them at your place, and the mouse
  wheel or a click on the rows goes through the rest.
- A fight starts with your group's first hit and ends ten seconds after its last; the meter keeps the last fight
  on screen until the next one. Click or tap its title to switch between the fight and Overall (every fight since
  you logged in), and Reset to clear it. Or type /dps (and /dps fight, /dps overall, /dps reset, /dps on,
  /dps off). With a controller: Options > Extended > Combat has DPS Meter Shows and Reset DPS Meter.
- It counts the damage numbers you are shown, your pet's included (and a necromancer's zombie, mummy and
  spectre, whose hits show no numbers), training dummies and duels too; heals do not count.
- It needs a server that keeps a meter (eqoaclassic's, once it is updated for it). On any other server nothing
  is shown.

## 2026-11-10 · Creatures in a server's own colours
- A server can now give a creature a colour of its own, laid over its whole body, where the game has no body in
  that colour: eqoaclassic's raid dragon Yhafiran, the black and green oriental dragon, is drawn nearly black,
  the green of his scales showing, though he wears the same green dragon as the Plane of Disease's twins. Each
  creature of the name is coloured, whichever spawn it is, a respawn too; what it holds keeps its own colours,
  and the light, the time of day and the fog fall on it as on any other.
- Every other creature and character looks exactly as before. On a server that colours none (Sandstorm, or one
  not updated) nothing changes, and a PS2 on the same server sees the plain green dragon.

## 2026-11-10 · Macros on a page of their own, as PCSX2 has them
- Macros have their own page: Options > Macros, after Controls (in the title's Options and in play). The
  sixteen macros are listed on the left with what each presses; choose one and set it on the right, laid
  out as PCSX2's macro editor is.
- A macro's trigger is any key, controller button, stick pushed one way or L2 or R2 pulled, or several of
  them held together (a chord, up to four: L1 and ✕, say). Choose Trigger, hold them and let one go: it is
  kept. Add a Trigger gives it another (up to four, any one of which runs it), □ (or Q) takes the one
  you are on off, and Clear Triggers takes them all off. A key a row of the Controls page uses is refused;
  a controller button you put in a trigger no longer does what it did in the game, until you give it back
  on the Controls page or take the trigger off.
- What it presses, all at once: tick any of the game's buttons and the sticks' eight directions on the grid.
- Press To Toggle: on, one press of its trigger starts the macro and the next stops it; off, it runs while
  you hold its trigger.
- Frequency: Off holds what it presses; N lets go and presses again every N frames as PCSX2 counts them
  (60 a second). The game reads its pad 30 times a second, so 2 is fifteen presses a second, 4 seven and a
  half, 30 one; the same at any frame rate.
- Pressure: how far it pushes a stick (under 24% the game reads the stick at rest, from 24% you walk, from
  60% you run). Deadzone: how far a stick or trigger in its trigger has to go before it counts.
- As in PCSX2, your controller and the macros press one pad, and whichever changed a button last has it:
  a macro letting go of ✕ lets go of a ✕ you are holding too, until you press it again.
- A macro runs at the game's own frames, so it can come up to a thirtieth of a second after its trigger,
  as on a PS2.
- Macros now work with no controller plugged in (from the keyboard alone they pressed nothing).
- Macros you made before keep working: a key and a button become two triggers, and a turbo keeps its speed.
  The macro rows have left the Controls page, and its Defaults now leave your macros alone (but for the keys
  and buttons it gives back to the game).

## 2026-11-10 · Light Shadows: soft, steady edges
- With Light Shadows on, the shadows of your lantern and of torches, braziers and lamps have soft edges now.
  Up close they showed little steps along every edge, and while your lantern moved (or anything walked past a
  fire) their edges crawled and flickered a step at a time; now they glide.
- A glowing weapon or a spell no longer makes the shadows of what is round it shimmer while you stand still: they
  hold still until the glow itself moves away.
- In a page and on Android the shadows are softer and steadier too.
- It costs the graphics card a little more where many lights overlap (on an RX 580 about 0.4 ms a frame at
  1080p in Freeport's torch-lit courtyard).

## 2026-11-10 · Helms, gloves and boots on every body of the new races
- On a server with races of its own, a helm now shows on every body those races can choose: giants, goblins,
  orcs, gnolls, lizardmen, minotaurs, kobolds, ratmen, kerrans, aviaks, frogloks, kappas, satyrs, brownies,
  dryads, gargoyles, the pixie and the fairy, each kind of body in every colouring and outfit. It sits on the
  head the way it sits on a human, the face showing, with a snout, beak or muzzle through the front and ears,
  horns and crests through the top. The fungusmen still wear none: their cap is their head.
- The bodies that showed only part of their armour (a chest, bracers and legs, but bare hands and feet: the
  frogloks, kerrans, aviaks, lizardmen, gnolls, orcs, kobolds, ratmen, satyrs, and the goblin warlord's and
  warmaster's arms) now show gloves, boots and bracers too, laid on the way they are on the races that showed
  nothing. Whatever these bodies already showed looks exactly as before.
- The hooded ratman shows its armour and robe (only its bracers showed), and the ratman trickster its gloves
  and boots.
- A fairy's shield is the size of its hands (it was more than three times as big as the fairy).
- A goblin's cape fits it: a goblin stands crouched, and its cape hung down to the ground; it now ends above its
  feet. Every other race's cape is exactly as before.
- Only players' characters are dressed this way: creatures and NPCs look as they always have.

## 2026-11-10 · The Homelands end in rocks, not a fall
- The Homelands' lands no longer end at a drop into nothing: every edge of a homeland that does not lead into
  another one is closed by a ridge of the game's own rocks (Odus's mossy boulders), on land and out in the
  water alike, so you cannot walk or swim off a homeland any more.
  Where two homelands touch (Odus's homes, the coast by the Warrens) you still walk from one into the next.
- A server's world can now be decorated with the game's own art, placed where the server says: your client
  (or the web page) copies the pieces off your own disc into the world it builds, as it builds the rest, and
  draws and collides them like everything else there.
- An older client cannot build the Homelands any more: its characters of the new races start in their mainland
  cities until it updates (which it does at its next start).

## 2026-11-10 · The Homelands in the browser
- The web page now builds a server's own worlds from your disc as the program does. The first time you reach
  character select on a server that has them (the Homelands, where a server's new races start), the page builds
  the world into your browser's own storage, a part of your disc at a time, and says how far it has come at the
  top of the page (a few seconds; nothing of the game is downloaded). Your next visits find it kept and only
  check it.
- So in the browser too a new Aviak hatches in Aviak Village, and every race with a homeland starts there,
  instead of in its home city. A character you play while the world is still being built waits for it.
- It takes about 130 MB of the browser's storage. A browser that cannot keep it (a page without its storage,
  some private windows) plays as before: a character from the Homelands starts in its home city.

## 2026-11-09 · A server's own worlds, built from your disc
- A server can now have worlds of its own made of the game's own lands: the Homelands, where a server's new races
  start (an Aviak hatches in Aviak Village). The client builds such a world from your own disc the first time you
  reach character select on that server (a few seconds; nothing of the game is downloaded), keeps it, and then
  plays it like any other world. The start menu names its places, and the create page's review shows a new
  race's home as its Starting City.
- Nothing changes on a server without such worlds.

## 2026-11-09 · Damage numbers in your colours
- The numbers that float up in a fight can be coloured by what made them, so you can see at a glance that a
  proc fired or a heal over time ticked. It is in Options > Extended > Number Colors: Your Damage, Your Procs,
  Your Heals, Pet Damage, Pet Procs and Pet Heals, each Default or one of eleven colours (White, Yellow,
  Orange, Red, Pink, Purple, Blue, Light Blue, Cyan, Green, Gray), shown in its own colour on the page.
- Your Damage is your swings and bow shots, your spells and abilities, your damage over time and your damage
  shield. Your Procs is everything a proc does: a weapon's, a buff's, a worn item's, its hits and its heals.
  Your Heals is your heals and heals over time, on yourself or anyone else. Your pet's three rows are the same
  for your pet, its heals on you included.
- Default is the game's own colour (white for a hit you made, green for a heal), and every row starts on
  Default, so nothing changes until you pick a colour. Numbers on you that others made (a hit you take, a heal
  someone else gives you) always keep the game's colours.
- It needs a server that marks its numbers: eqoaclassic's does from its update of the same day. On any other
  server (Sandstorm, or one not updated) your numbers stay the game's colours whatever the rows say.

## 2026-11-09 · No cape on a four-legged form
- A cape is no longer drawn on a character turned into a four-legged animal (Leopard Form, the bear, wolf,
  lion and panther forms, a rat): it hung from the animal's tail. Two-legged forms (a were, a golem, the
  illusions of the races) still wear theirs.

## 2026-11-09 · GMs: every trigger in the TEST menu
- For a GM, TEST > Programming > TRIGGER now lists every trigger that is switched on, each with its number
  before its name ("140 Marr Twins..."), not only the first eight. Up and Down go through them seven at a time,
  as the game's other long menus do (Up from the top goes to the last one), and the mouse wheel and a click
  work there too.
- Picking one fires that trigger, whatever its number, as `#trigger <number>` does. A trigger made, changed,
  switched off or started from the GM panel shows in the menu at once, even while it is open.
- On a server that names only the eight lines, the menu is as it was.

## 2026-11-09 · Your pet's buffs show under your pet's name
- The buffs on your pet (its own, and the ones you cast on it) show in a small row under your pet's name and
  health, the same icons as your own buffs at half their size, and each goes away when it wears off. With the
  server's update of the same day they no longer show in your own buff row next to the compass, on any client: that
  row is your own buffs only again.
- Clicking your pet's buff row targets your pet, as clicking its name does.
- It is in Options > Extended > UI as Pet Effects, on by default. Off, your pet's buffs show nowhere, as on a
  PS2. On a server that does not send them (Sandstorm), there is no row.

## 2026-11-09 · The Red Swarming Death's swarm looks as it does on a PS2
- A necromancer's Red Swarming Death (and the Swarming Death, the Construct of Sorrows) is a cloud of red puffs
  round a faint horned skull. Its puffs used to pile up into one dark red blot; now they are drawn as the PS2 draws
  them, each faint puff stopping short of the next, a speckled red swarm you can see through, as on a console.
- The same goes for the other faint particles of that kind: the flies and stench round the rotting creatures
  (flesh fiends, rats of pestilence, the dead riders), smoke, rain and snow, and the dark auras some spells cast,
  which come out darker and speckled, as on a PS2.
- It is in Options > Extended > Lighting > Particles: PS2 (the game's look, the default) or Exact (as before,
  smoother and darker where many overlap). The fires and glows are unchanged.

## 2026-11-09 · Weapons cast shadows standing still
- With Light Shadows on, at night your weapons now cast a shadow from your lantern while you stand still, as they do
  while you move. A sword held across you at rest was being taken for your body (which your own lantern never
  shadows) and cast nothing. Shadows from the sun and from torches were not affected.

## 2026-11-09 · The on-screen controller: its size, opacity and edge
- On a phone or a tablet (the browser page and the Android app), Extended > Controller has four new rows for
  the on-screen controller. Touch Stick Size and Touch Button Size set the two sticks and the buttons apart,
  from half their size to twice it; Touch Opacity makes the whole controller fainter or brighter (10% to 200%);
  Touch Edge keeps it farther in from the screen's left and right sides (up to a tenth of the screen).
- Larger sizes use the room the screen has: on a phone held sideways the sticks go to twice their size and the
  buttons to about two-thirds larger, the sticks moving in toward the middle to make room. A tablet, or a phone
  held upright, has less room; where a size you pick does not fit, the controller is drawn as large as fits
  and the options page says the size it is drawn at. No control ever goes off the screen or onto another.
- Nothing changes unless you change these rows. They replace Touch Pad Size: if you had set it, both sizes
  start where it was, so your controller looks just as it did.

## 2026-11-09 · Capes over broad shoulders and round the neck
- On the big race bodies (a minotaur's hunched shoulders, say) a cape's collar now comes up over the shoulders
  instead of sinking into them, standing and in the battle stance.
- The vampire's cape and its mantle sit on the shoulders again: the last update stood the mantle out wide and
  forward round some bodies, leaving a big opening at the neck. It now closes round the neck and lies on the
  shoulders as it does on the vampire himself, and walking and running lift it only a little.

## 2026-11-09 · Fewer quest marks, and GMs travel by the map
- The minimap keeps only your own quests' "?" on its rim, pointing the way to them. A "!" (a quest you can take)
  shows on the minimap only where it lies within what the minimap shows, so its edge is no longer ringed with them.
  On the map, a "?" is drawn over a "!" beside it.
- On go-eqoa-server servers (eqoaclassic's) the "!" marks are far fewer: a quest given in every city, such as
  Armorcrafting and Jewelcrafting, is marked once, at its nearest giver, and not at all while you have it under way
  or once you have done it; only quests near your level are marked (from 10 levels below yours, as the server
  sets it); and a giver where a step of yours is to be done shows that step's "?" alone.
- GMs: with the map up, Ctrl and a click (Cmd and a click on a Mac) takes you to that spot, on the ground there;
  the map's legend says so while you are a GM. Anyone else's click puts a waypoint, as before.

## 2026-11-09 · The Jal'Raeth and the Wisp
- Two more races on a server that has them. The Jal'Raeth float cross-legged: choose their robes, a channeler's
  body or a knight's armour. They wear chest armour, bracers and gloves as a human does (their crossed legs keep
  their own garb), their helm sits on their head, their weapons float at their side, and they wave, bow, point
  and cheer.
- A Wisp is a light inside an empty suit of armour: you see only what it wears, shaped as a human would wear it,
  its weapons swinging as a human's do, and the wisp glowing inside the helm. With nothing on, it is the wisp
  alone, floating where a head would be. Other players with this client see it so; a PlayStation 2 shows a human
  in the same armour.

## 2026-11-09 · Light Shadows: no square under lamps, steady weapon glows
- With Light Shadows on, the ground under a street lamp is lit again. The lamp's light sits inside its little
  housing, and the housing's floor was darkening a square of ground under every lamp.
- A glowing weapon (an epic sword's glow) or a spell's light no longer makes the shadows of the person or weapon
  carrying it flicker on and off. Those lights still cast the shadows of walls and everything else that stands
  still, steadily; torches, braziers and your lantern still cast everyone's shadows as before.

## 2026-11-09 · Torches, lamps and spells cast shadows
- A new option, Extended > Lighting > Light Shadows (off unless you turn it on; like Sun Shadows it needs Dynamic
  Lighting on): the light of torches, braziers, candles, lamps, your lantern and spells now stops at whatever stands
  in its way. A character standing by a brazier casts a shadow away from it, a stool by a torch casts its own, and a
  torch's light no longer comes through the wall it hangs on into the room behind.
- Each moment the brightest and nearest lights cast shadows (eight on a computer, four in a browser and on a
  phone). When another light takes one's place, the old shadow fades out and the new one fades in over a quarter of
  a second, so no shadow pops in or out.
- Your own lantern does not shadow the ground round your feet; the people and things near you cast shadows away
  from you as you walk.
- Shadow Distance, a slider beside them: how far out shadows reach. At its left, where it starts, they reach as far
  as before; to the right the sun's shadows reach up to four times as far (1000) and more of the nearby lights cast
  at once, up to every lit torch and lamp you can see (sixteen on a computer, eight in a browser and on a phone).
  Farther costs more of your graphics card.
- With it off, the game looks exactly as it did.

## 2026-11-09 · Texture packs load without freezing the game
- A texture pack's pictures are now read in the background. Until a texture's replacement is ready the game draws
  its own texture there, then puts the replacement in its place a moment later, so the game no longer stops while a
  pack is read. With a large HD pack the game used to freeze for seconds on arriving in a city like Freeport; now
  it keeps moving.
- Dump Textures only ever writes the game's own textures, as the disc has them, whether a pack is loaded or not:
  never a pack's picture and never an enlarged one.

## 2026-11-09 · Texture packs in a folder beside the game
- Texture packs and texture dumps now go in a `textures` folder next to the game itself (beside `eqoa.exe` on
  Windows, beside `eqoa` on Linux and on a Mac), where they are easy to find: put a pack's files in
  `textures/SLUS-20744/replacements`, and Dump Textures writes to `textures/SLUS-20744/dumps`. Inside it the folders
  are PCSX2's, so a pack made for PCSX2 still goes in as it is.
- Options > Extended > Textures has a new row, Texture Folder: Open shows the folder in your file manager
  (Explorer, the Finder or your desktop's). Beside Back the page says where the folder is, and how many textures
  the packs in it replace. A pack you add while the game runs is read when you turn Texture Packs off and on.
- The packs and dumps you kept in the settings folder (`eqoa-rs`) are moved there the first time the game uses
  them. If they are on another disk from the game they stay where they are, and are still read.
- Where the game cannot write in its own folder (in Program Files, say), it keeps them in the settings folder as
  before, and the Textures page says so.
- An update replaces only the game's program: the `textures` folder beside it is never touched.
- In a browser and on Android nothing changes.

## 2026-11-09 · The map and the quest helper are back
- M opens the map again: the whole world drawn from your own disc, with the towns' names, the coach routes and
  stops, the XP camps and the places players suggested and a GM accepted. Click (or tap) to put a waypoint, which
  shows on the compass's rim wherever you are; in a group, O (or Q held, or Shift and a click) tells your group
  where to meet, and their map and compass show it. G stays the attack.
- The minimap is back under the quick menus (N shows or hides it), with your group, the creatures and players
  near you, your target and your waypoint. Click it to open the map; the mouse wheel over it zooms.
- The quest helper is back: a gold "!" on the map and the minimap at each quest giver you can take a quest from
  now, a "?" where a step of a quest you have is to be done, each with a card of the quest, and the quest tracker
  under the minimap ("slashed pawn belt: 0/2"); click the tracker to open your quest journal. On a server that
  does not send quest marks (Sandstorm), the client works them out itself, marked "(guide)".
- Both are in Options > Extended > Map, on by default: Map (the map, the minimap and the waypoints), Minimap,
  Map Markers, Quest Helper (the quest marks, the tracker and the guide) and Quest Tracker. Turn the Map and the
  Quest Helper off and the game is as it was without them.
- The Stats Box, other players' mana bars and Target of Target are not coming back.
- A macro you put on M, N, O, =, +, - or _ (free keys until now) keeps its key: that map key is then
  left without one, and you can give it another on the Controls page (F1). A key you moved there keeps it too.

## 2026-11-09 · A crash in the browser comes with your next bug report
- If the game stops in the browser ("The client stopped" or "The graphics stopped"), the next bug report you send
  (F3, or /bug in chat) carries the message it stopped with, once, so the GMs can see what went wrong without you
  copying anything. Nothing is sent until you send a report yourself.
- A bug report from the browser now says which build of the page you were on.
- When chat lines can't go out (the server reset your connection in the world and hasn't sent the world again), the
  game still sends nothing for them, as the PS2 does, but your next bug report says how many were held back.
- A crash that could come when the server's connection broke in a particular way (the game stopped the moment
  you moved) can't happen any more: the game carries on without that connection and says so in its log.

## 2026-11-09 · Posts no longer pull the camera in
- Turning the camera with something thin close by, like the posts holding up the roof of the well by the Freeport
  coach, a lamp post or a pole, the camera no longer jumps in each time one passes behind you: it stays where you
  had it, zoomed out as far as you like, and the post simply passes across the view. Walls, roofs and anything broad
  still stop the camera as before.

## 2026-11-09 · Every body of the new races
- The new races' Customize Character page now offers every other body of their creature on the disc, the named
  ones' too: goblin kings, overseers, priests and bandits; grey, black-and-tan and risen gnolls; Deathfist and
  hatebone orcs (three of them women's); two more lizardmen, a ratman trickster, three aviaks no one in the world
  wears, the fire giant reaver and King Daran, the great skeletons, a brown minotaur, and the dryad's erych and
  haliai kin. Each stands as tall as its race.
- Hair colour where the body has hair: goblins, orcs, kappas and the dryad's kin, and a kerran's mane.
- A body you choose waves, bows, points and cheers as its race does, and a great skeleton wears armour round
  its bones as a skeleton does.
- Character select shows a character you just made in the body you chose (it showed the race's usual body
  until you looked at another character).
- Characters made before keep their look.

## 2026-11-09 · A Skeleton's helm covers its skull
- With the 2002 disc's art on (F9), a Skeleton's helm no longer shows the back of its skull poking out
  through it. The helm now sits a little wider and deeper over the skull, as the Frontiers art's helm
  always has, so the whole skull stays inside it from every side and in every move.
- The old, mossy and frost bone skeletons a Skeleton can choose wear their helms a touch further forward
  in the Frontiers art too, so their cheekbones stay inside the helm's sides, and so do the great skeletons,
  with either art.
- Every other race's helm, and a skeleton creature's, is exactly as before.

## 2026-11-09 · Capes sit clear of armour
- A cape now sits off what you wear: plate, broad shoulders and a robe's skirt show through it far less as you
  stand, walk and run. Over the back and shoulders it moves with the body under it, and under a robe its lower
  part swings with the robe's hem.
- Wherever your race's walk or run would still bring the body through it (a broad back, an arm swung back), it
  hangs a little further off, only there and never by much; where nothing comes near it, it rests where it did.

## 2026-11-09 · The camera at posts and low roofs
- Steering with the right mouse button (or with the camera tilted low), the camera no longer jumps all the way in
  to your character when something close behind you is in its way, like the posts and roof of the well by the
  Freeport coach. It comes in only as far as it must, just in front of what is in the way, and goes back out to
  where you had it once the way is clear.

## 2026-11-09 · Jump on Space, and a fall from the top
- Space jumps now, on a server that has jumps (the Heirloom Test Server), and V still does. The attack (hold to
  attack, Auto Attack's toggle, a corpse's loot) is on G.
- Wherever Space makes no jump it still does what it always did: on a server without jumps (the Live Server,
  Sandstorm, eqoa.live) Space attacks, and with a window up it takes from a corpse, opens a trade's chat line and
  casts a fishing line, as before.
- Keys you moved on the Controls page (F1) stay where you put them. A macro you had put on G keeps G; the attack
  then has no key until you give it one there.
- Other players using this client now see your jump as you do: from the top of the arc you fall, and you land with
  your landing when you come down. Before, they saw the rise and the top standing or running, and no landing.
- The new races whose bodies had no fall of their own (froglok, giants, kerran, goblin, aviak, lizardman, kappa,
  gnoll, kobold, minotaur, ratman, satyr and gargoyle) now fall and land as a human does, fitted to their size;
  pixies, fairies and fungusmen use the nearest moves of their own, as their emotes do. Before, they stood in the
  air. That is on a server with jumps, in a jump or a fall off a ledge, for you and for others using this client;
  a PlayStation 2 still shows them standing.

## 2026-11-09 · Spells' lights no longer flash
- With Dynamic Lighting on, the light a spell throws on the ground and the walls round it now changes smoothly.
  Some spells' light flickered many times a second, jumping in brightness and in colour (white to blue and
  back) over a wide circle, which could be hard on the eyes and a risk for anyone sensitive to flashing light.
  A spell's light now swells in as the spell starts, holds steady while it plays and fades out as it ends: it
  never flashes more than three times a second, the limit photosensitivity guidance sets.
- Spells still light their surroundings in their own colours; only a lightning bolt's light, which turned
  everything round it full white, is toned down a little. The glow of an epic weapon and the light of a fiery
  creature are steadied the same way.
- Where more lights are about than the game draws at once (a town's torches at night when a spell goes off), a
  torch that gives up its place to the spell's light now dims out over a moment instead of going dark at once,
  and comes back the same way.
- Torches, braziers and lamps look as they did. With Dynamic Lighting off nothing changes.

## 2026-11-09 · A pack and a bank of 100
- On a server that has them (the Heirloom Test Server), your inventory holds 100 items and your bank 100, where the
  game holds 40 of each. What you wear counts among the 100, as it counts among the game's 40.
- Every window shows them all: the inventory's tabs, the bank, a merchant's list when you sell, a trade, a corpse,
  the forge and the auction's bank list. They scroll as they always have: hold up or down, or turn the mouse wheel;
  up from the first row goes to the last.
- On a server without them nothing changes. A character carrying more than 40 who logs in on a PS2 sees its first 40
  (what it wears first), and the server keeps the rest safe; they come back when you log in here again.

## 2026-11-09 · Jump
- On a server that has jumps (eqoaclassic can turn them on), V jumps: about half your height, straight up when you
  stand and on along your way when you run. You come down as a fall comes down and land where you came down, and
  others playing this client see you rise and land.
- The jump is on the Controls page (F1), where you can move it to another key. Every button of a controller is
  already one of the game's, so a controller has no jump button until you give it one there.
- No jump in the water, in the air, while casting, frozen or rooted, or with a window up. A server without jumps
  (Sandstorm, eqoa.live) never offers one, and there V does nothing. A PlayStation 2 cannot jump: it shows a
  jumping player on the ground, playing the fall for a moment.

## 2026-11-09 · Customise the new races
- On a server with races of its own, the Customize Character page now offers each new race its own choices
  instead of faces and hair it cannot wear: another kind of the same creature (hill, ice, sand and fire giants and
  cyclopes; grey, green, ice and fire goblins; hawk, raven, vulture and owl aviaks; tiger and lion kerran; red and
  pale fungusmen, and more), a satyr's face and hair, a brownie's hair, and colours for a fungusman's skin, cap,
  growths and bag, a pixie's or fairy's wings and dress, a minotaur's hide and horns, a dryad's bark and leaves and
  a giant's whole tone. Left and right step through them on the model, as with faces and hair.
- What you choose shows at character select and in the world, to every player using this client. The kind of body
  shows on every client, a PS2 included; the colours, faces and hair show on this client.
- Characters made before keep their look, and every other race looks exactly as it did.

## 2026-11-09 · Capes
- On a server that has capes (the Heirloom Test Server), your character can wear one: a cape goes in its own
  slot, so it is worn with everything else. Equip it from the Equipment tab like any other piece; its
  description says Slot: Cape.
- Capes are drawn on you, on the other players around you and at character select. They hang from the
  shoulders and move with the body: they sway as you walk and run, and fold when you swing.
- Five looks, borrowed from capes the game's own characters wear: a knight's long blue silk cape, a
  vampire's black cape with a high red-lined collar, a long pleated violet cape, Dread's maroon cape with
  its tall collar, and the same with Dread's fur mantle over the shoulders. Some come in other colours.
- Every race can wear one, the server's own races too, but those with wings (pixies, fairies and gargoyles).
  On a server without capes nothing changes.

## 2026-11-09 · A Skeleton's helm shows in the 2002 art
- With the 2002 disc's art on (F9), a Skeleton now wears its helm in the world, as character select
  already showed it. The 2002 disc's own skeleton has no place on its head for a helm, so the helm
  went unseen in the world; it now sits on the skeleton's skull, where a human wears it.
- In the 2002 art, character select and character creation now show a race's body the way the world
  shows it: the 2002 disc's own Skeleton, Goblin, Lizardman, Brownie, Gnoll, Orc and Minotaur, where
  they showed the Frontiers bodies before. A Goblin's or an Orc's helm, which their 2002 bodies wear
  in the world, now shows in character select too. With the Frontiers art nothing changes.

## 2026-11-09 · A Skeleton's armour is solid from every side
- The armour a Skeleton wears round its bones no longer lets you see through it. Looking down at its
  collar, or in through the gaps of chain and cloth, you now see the inside of the armour, not the
  ground behind it.

## 2026-11-09 · Sun Shadows move smoothly
- Shadows now move with the sun a little every frame as the day goes on. Until now they stood still for a second or
  so and then jumped to their new place, which showed most on the long shadows of the morning and the evening.
- The shadows of characters walking or running some way off keep up with them now. Farther than about 40 from the
  camera they were redrawn only every other frame, so they trailed a step behind and flickered over the character's
  own sunlit side.
- Sun Shadows ask a little more of the graphics card for it: about a third of a millisecond a frame on a Radeon RX 580,
  at 1080p and at 4K alike.

## 2026-11-09 · The new races wave, bow, point and cheer
- On a server with races of its own, every new race now does the four emotes. Most do them the way a human
  does, in their own size and shape: a Hill Giant raises its huge arm over its head to wave and bends right
  over to bow, a Kerran points, a Gnoll cheers with a fist in the air.
- Pixies, Fairies and Fungusmen, built too differently for that, use moves of their own: a Pixie raises a hand
  high to wave and dives forward to bow, a Fairy opens its wings, a Fungusman swings an arm out and tips its
  cap.
- Other players see your emote as you do. Until now a new race's emote did nothing at all (it started its
  standing animation over), which is still what a PS2 shows.
- Brownies and Skeletons, which always had the human's emotes, and everyone else, creatures included, are
  unchanged.

## 2026-11-09 · Longer bug reports
- A bug report or petition now holds up to 2,047 characters on a server that takes them: Support's request form,
  the one F3 and /bug open, and a reply from Support's inbox. That is four times as much room to say what happened,
  where, and how to make it happen again. The box scrolls as you type, and Up and Down move through its lines.
- On a server that does not take them it holds 511 characters, as on a PS2.
- Typing in a text box and pressing Up or Down straight after could close the game. It no longer does, and a long
  text no longer slows the game down while its box is open.

## 2026-11-08 · The mastery window can't hang
- If the server's answer to a mastery request (buying a mastery, opening one, your share of experience) is lost on the
  way, the window's "please wait" box now lets go after ten seconds instead of staying up for good with no way out.

## 2026-11-08 · Sun Shadows
- A new option, Options > Extended > Lighting > Sun Shadows, lets the sun cast shadows. Buildings, walls, roofs,
  trees and their leaves, rocks, hills and every character block the sunlight, and their shadows fall across the
  ground, the walls and each other. They follow the sun through the day: short at noon, long in the morning and
  the evening. Step under a roof, an awning or a tree and you stand in its shade.
- It works with Dynamic Lighting on (the page tells you when it is off). The shadows reach a good way from the
  camera and fade out beyond it. At night there are none: the game's night light comes from under the horizon.
- It is off unless you turn it on, and off the game looks exactly as before.

## 2026-11-08 · eqoaclassic.com is the server out of the box
- A new install (on a phone too) connects to eqoaclassic.com without being told: the title's Network Setup shows it, and
  logging in works at once. Until now a client that had never been given a server had none, so a phone's first login
  could not reach anyone.
- A server you typed in Network Setup is kept as it is, and so is a blank one (none). The browser page is unchanged: it
  is told its own server.

## 2026-11-08 · Fungusmen keep their bag and fungus, skeletons wear real armour
- A Fungusman in armour keeps its own bag and strap, and its glowing fungus: the growths on its
  shoulders, wrists and shins, its claws and its cap look as they always did, and the armour covers
  the rest of its body. Its upper arms now show the armour's sleeve, shoulder plate and all, instead
  of a stretched, smeared piece of chest plate.
- A Skeleton now wears its armour the way a human does, round its bones, instead of armour painted
  thinly onto each bone. Only what it wears is shown: a chest piece alone leaves its bony legs, hands
  and feet bare.
- As before, only players' characters are dressed this way, and every body that already showed
  armour looks exactly as it did.

## 2026-11-08 · Ambient Occlusion
- A new option, Options > Extended > Lighting > Ambient Occlusion, shades the places the light reaches least: where a
  wall meets the ground, the corners of a room, under a ledge or the edge of a roof, round a barrel, a post or a tree
  standing on the ground, and under the feet of everyone standing about. The world looks more solid and its objects
  sit on the ground instead of floating over it.
- Water, glows, fog, spell effects, shadows, names and the HUD are drawn over it as they always were, and it fades out
  in the distance and into the fog. It works with Dynamic Lighting, Render Scale and both Ground Fog settings, in a
  browser too.
- It is off unless you turn it on, and off the game looks exactly as it did on a PS2. On, it asks a little more of
  the graphics card: under a millisecond a frame at 1080p on a Radeon RX 580, and about one and a half at 1440p or
  4K.

## 2026-11-08 · Gargoyles hold their weapons
- On a server with races of its own, a Gargoyle now shows what it holds: its weapons in its claws and its
  shield on its left arm, at character select and in the world. The gargoyles' body was made for creatures
  that carry nothing, so a PS2 draws no weapon on it either; now a player's Gargoyle carries its gear as the
  other races do, and a bow in hand shoots its arrows. Its swings are still its own claw attacks.
- Only players' characters change: gargoyle creatures look as they always have, and so does a player
  turned into a creature by a form or an illusion. Every race that already showed its weapons looks
  exactly as before.

## 2026-11-08 · Dynamic Lighting
- A new option, Options > Extended > Lighting > Dynamic Lighting, lights the world as a PC game would: the light of
  the sky and of your lantern is worked out for every pixel instead of only at the corners of each surface, so it
  falls smoothly across the ground and the walls.
- With it on, the world's own fires light what is round them: torches, braziers, candles and street lamps throw a
  warm pool of light onto the ground and onto the characters walking past, flickering a little as a flame does.
  Spells light up their caster and the walls near them in their own colour while they play, and so do glowing
  weapons and fiery creatures.
- It is off unless you turn it on, and off the game looks exactly as it did on a PS2.

## 2026-11-08 · Armour shows on the races with creature bodies
- On a server with races of its own, the Kappa, Brownie, Pixie, Fairy, Minotaur, Dryad, Skeleton,
  Fungusman and Gargoyle (and the Hill Giant's own body) now show the armour and robes they wear. Their
  bodies were never made for armour, so nothing they wore was drawn; the armour is now laid over their
  torso, arms, hands, legs and feet the way it sits on a human, in its own colour and dye. Heads, wings,
  tails and the like keep their own look.
- Only players' characters are dressed this way: creatures and other NPCs look as they always have.
  Every body that already showed armour (the game's races, Kerrans, Goblins, Frogloks and the rest)
  looks exactly as before.

## 2026-11-08 · Texture packs and texture dumps, as PCSX2 makes them
- Texture packs made for PCSX2 now work here as they are: put the pack's files in the `textures/SLUS-20744/replacements`
  folder of the client's settings folder (`eqoa-rs`: in `~/.config` on Linux, `~/Library/Application Support` on a Mac,
  `%APPDATA%` on Windows; it is made for you the first time), then turn on Options > Extended > Textures > Texture Packs.
  PNG and DDS files of any size are read as PCSX2 reads them, subfolders too; a larger texture keeps its detail up close
  and looks as the game's does in the distance.
- Dump Textures, on the same page, writes each texture the game draws to `textures/SLUS-20744/dumps`, named as PCSX2
  names it, so a pack made from them works in PCSX2 as well (and a pack made from PCSX2's dumps works here).
- Both are off until you turn them on, and the game looks exactly as before. Not in a browser.

## 2026-11-08 · Every class for the newest races
- On a server with races of its own, character creation now offers classes for all of them. The races
  numbered past the game's own twenty (Orc, Kobold, Minotaur, Ratman, Satyr, Dryad, Skeleton, Fungusman and
  Gargoyle on the Heirloom server) showed no class to pick; they now take every class the server allows.

## 2026-11-08 · Scroll a spell description with the mouse or the keyboard
- A spell description too long for its box now scrolls on the mouse wheel over the box, and on PageUp
  and PageDown, as well as on a controller's R1 and R2. Over the list, the wheel still moves the
  selection as before.

## 2026-11-08 · Scroll a long spell description
- In the spell book, R1 and R2 scroll a description too long for its box, a line at a time (R1 up, R2
  down), as the live game did. It goes back to the top when you pick another ability.

## 2026-11-08 · /bug opens the bug report, raid frames show power, unmemorize one ability
- Typing /bug in chat now opens the bug report form (as F3 does), already set on Bug Report. Anything you
  type after /bug is put in the form's text, so you can finish it there and send it. On a server without
  bug reports, /bug goes to the server as before.
- The raid list shows each member's power as a thin blue bar under their health. Each row is a little taller,
  so a column holds 12 members (13 before) before the list starts another.
- The spell book's menu has Unmemorize: it takes the selected ability off its bar slot and leaves the rest,
  where Unmemorize All clears them all.

## 2026-11-07 · Your combat record
- On a server that keeps one, the start menu's Character has a new entry, Combat Record: your PvP kills,
  deaths and assists, your kill streak and your best, the highest-level player you have killed, your last
  kill and who last killed you, and your duels won, lost, surrendered and drawn, with the tunar your wagers
  won and lost. Triangle (or Esc) goes back to the start menu.
- It counts from when the server started keeping it, for each of your characters on that server. In chat,
  /record shows yours and /record with a name anyone's.

## 2026-11-07 · More races to make
- On a server that has them (the Heirloom Test Server), character creation offers more races: the froglok,
  hill giant, kerran, goblin, aviak, lizardman and kappa, each in the list in its alphabetical place with
  its own description, bonuses, body and starting city. Your character stands in the arena in its own body
  while you choose, and at character select too.
- Such a server can also open every class to every race: every class is then lit on the class page,
  whatever the race.
- Nothing changes on other servers, or on a server that doesn't offer them.

## 2026-11-07 · Three-letter names
- A new character's name may be three letters long (it was four). It is still letters only, A to Z.
  A PS2 keeps asking for four.

## 2026-11-07 · Auto Attack on from the start
- New players now start with Auto Attack on (Options, Game). With it off, the attack button
  has to be held down to keep fighting: a tap swings once and stops, which made a first fight
  look like a laggy server. If you already saved your options, your own choice stays; you
  can turn it either way there as before.
## 2026-11-07 · The licence
- The client's source goes under the GNU Affero General Public License (version 3 or later) in place of
  MIT, from the next build on. Nothing changes in playing it. If you hand out a changed copy or host a
  changed page, players have to be offered its source. Earlier versions stay under MIT.

## 2026-11-07 · GMs can fly
- For GMs, on a server that has the command: `/fly` turns flying on, and `/fly` again turns it off (or
  `/fly on` and `/fly off`). Nothing changes for anyone else.
- While flying, moving forward at the camera's usual angle goes straight along and holds your height. Look
  up from there to climb and down to dive, at your ordinary run speed whichever way you go. In first person,
  looking level is level. Let go and you stay where you are in the air. Your character runs and stands as it
  does on the ground, and walls and the ground still stop you.
- With a mouse, look up or down as usual. With a controller or the keyboard in the follow and orbit cameras,
  the right stick's up and down (Page Up and Page Down) look up and down while you fly, instead of zooming;
  the wheel still zooms. Your camera goes back as it was when you stop.
- Turned off, you fall and walk as normal from where you are.

## 2026-11-07 · The raid list
- In a raid group, on a server that has them, the group's bars become a list of everyone in it: each
  name over a thin health bar, in more columns as the raid grows, and past four columns the mouse wheel
  over it steps through the rest. It sits where the bars do, with your pet's bar under it.
- Click a name to target that member, when they are near enough for you to see. Someone out of sight is
  greyed, your target's name is gold, your own is blue and the dead are red. L1's cycle reaches
  everyone you can see too.
- Options > Extended > UI > Raid List turns it off, and you get the game's bars, as on any other server.

## 2026-11-07 · A server's own races
- A server can have races of its own (eqoaclassic's Hill Giant). Such a character now reads as its own race wherever
  the game shows a character's race: a target's details, your Stats page, the who list, the guild roster, the buddy
  list, the group search and a duel's invitation say Hill Giant, or HGT where the column is short, and not the race it
  plays as (Ogre, OGR). The who list and the roster sort by the word they show.
- On those same characters the title in a target's details no longer repeats the race: Titan, not Hill Giant Titan.
- Its camera, what it may wear and how it looks are as before. A server with no such race looks just as it did.

## 2026-11-07 · Picking the disc on a phone
- In a browser on an Android phone or any touch device, the disc image's picker now shows every file, as it
  already did on an iPhone. It could grey out an .iso before, so there was nothing to pick. The page still checks
  the disc you choose.

## 2026-11-07 · Copy and paste, and chat lines
- Any text box in the game (the chat line, Support and F3, mail, a tell, names) takes Ctrl+V to paste
  at the caret, Ctrl+C to copy what it holds and Ctrl+X to cut it (Cmd on a Mac), in the program and
  in a browser.
- Text you paste is kept to what the game's own keyboard can type: letters, digits, a space and
  ! " % ' ( ) * + , - . / : ; = ? @ _ ~. Other characters are left out, tabs become spaces, and curly
  quotes and long dashes become plain ones. Line ends stay in a mail's body and a Support message, and
  become spaces everywhere else. It stops at the box's own length.
- A password's box can be pasted into, but not copied or cut.
- Chat lines can be copied, in the program and in a browser, with a mouse. Press on a line in the chat
  box and drag over the lines you want: they light up. Drag past the top or bottom of the box to go on
  through the lines it has kept. Ctrl+C (Cmd+C on a Mac) copies them as plain text, a line for each
  row. Esc or a click anywhere else clears them.
- With nothing selected and no text box up, Ctrl+C copies the newest chat line.
- While the chat history is up (C or R2 held, or lines selected), the mouse wheel over the box steps
  through it. A plain click on a chat line still opens the chat line.

## 2026-11-06 · Macros
- The Controls page (Options > Controls, or F1 while you play) has sixteen macros after its keys. A
  macro presses a set of the game's buttons for you, all at once as a controller would, when you
  press its key or its controller button.
- On "Macro 1" (and 2 to 16), pick the key that does it, a controller button, or both. A controller
  button you put there does the macro and no longer what it did in the game. A key that a row of the
  page already uses is refused: move that row first.
- On "Macro 1: presses", choose ✕ and then press the buttons it should press, the controller's or the
  keys for them, together or one after another: a second after the last one they are kept.
  Backspace leaves it pressing nothing.
- Beside them, how it presses (✕ steps through the choices): Hold (while you hold its key or
  button), Toggle (on at one press, off at the next), or Turbo, pressing and letting go over and
  over, from 15 presses a second (Turbo 1) down to one a second (Turbo 15), as fast at any frame
  rate. A toggled turbo keeps going until your next press.
- Macros are kept with your keys when you Save Changes, and Defaults clears them. A server sees
  only the button presses, the same as from a controller.

## 2026-11-06 · Any group member can invite
- On a server that allows it (eqoaclassic does), anyone in a group can invite another player into it,
  not only the leader: from the target's menu or the who list, as the leader always could. The leader
  stays the leader. Elsewhere only the leader invites, as before.

## 2026-11-05 · Report a bug with F3
- On a server that takes bug reports (it tells you so when you log in), F3 opens Support's request
  form, the game's own, with Bug Report already picked and its topics open: choose what the bug is about
  (a quest, a creature, an item, a spell, combat, the game client or something else), say what
  happened, and send it. Send it from where it happened: the server keeps where you were, what you
  had targeted and what you had just been told, so the GMs can see what you saw.
- Typing /bug and what went wrong in the chat line sends one on the spot, the same way.
- A GM answers in Support's inbox in the start menu, and you are told in chat when one has.
- With the report goes a short note of this program's own: its build, your system and graphics card,
  the frame rate, and where it showed you and your target. Nothing else, and no account name. A
  server that does not take bug reports (Sandstorm, eqoa.live) is sent nothing, and F3 does nothing
  there.

## 2026-11-04 · 8BitDo controllers in the browser
- In Chrome and Edge on Windows and Mac, an 8BitDo controller in its D-input mode (an 8BitDo Pro 2
  with its switch on D, an SN30 Pro, a Lite 2 and their kin) now has every button where it belongs:
  B, the bottom button, is ✕, A is ○, Y is □ and X is △, L and R are L1 and R1, ZL and ZR are L2 and
  R2, − and + are Select and Start, and the d-pad works. Before, the buttons came mixed up (✕ on A,
  Start on ZR), the d-pad did nothing (so the Options window could not be stepped down to Save
  Changes without the keyboard or the mouse), and the right stick's up and down were read from the
  wrong place.
- Any other controller those browsers have no layout for now has its d-pad and right stick read
  where such controllers keep them, and no longer changes layout partway through a session: pushing
  its right stick up and to the left used to leave L2 and R2 held from then on.

## 2026-11-03 · A slow walk keeps walking
- Walking slowly with a controller no longer makes your character's legs switch between walking and
  running every moment when you head in some directions (on the way to Warlord Jurglash, for one).
  Your character now takes its steps on the same clock as a PS2, which also makes walking and running
  a little slower than before (about 2% when running), as fast as on a PS2.

## 2026-11-03 · Dying shows your body again; no stutter step on a light push
- When you die, the camera now starts where it was and swings round and down onto your body as on a
  PS2. It used to start pointing nowhere: the screen broke up into garbage (stripes, a smear of
  ground) for the first moments, longer the further out you had the camera zoomed, then jumped to
  looking straight down.
- Pushing the left stick partway, at the point between walking and running, no longer makes your
  character switch between a walk and a run several times a second. Once you push past that point you
  run until you ease off a little further back; held still, the stick walks or runs where it did.

## 2026-11-03 · Lighter work in the browser
- In the browser, the game's sums for placing and turning things (characters' bones as they move,
  collisions, the camera) now work on four numbers at once, as they already do on computers and
  phones, and take about a third less time. The results are the same to the last bit, so nothing looks
  or plays any differently.
- A browser too old for this (Safari before 16.4, so an iPhone or iPad not yet on iOS 16.4, or Firefox
  before 89) now says so when the page opens, and which browsers run the game.

## 2026-11-03 · Lighter work on Apple silicon Macs and on Android
- On Macs with Apple's own chip (M1 and later) and on Android phones and tablets, the game's sums for
  placing and turning things (characters' bones as they move, collisions, the camera) now work on four
  numbers at once, as they already did on Intel and AMD processors, and take up to a third less time.
  The results are the same to the last bit, so nothing looks or plays any differently.

## 2026-11-03 · The game on Android
- The game now runs on Android phones and tablets (Android 8.0 and later) and on Chromebooks:
  download eqoa-vanilla-android.apk from this page and open it to install it (Android asks you to
  allow installing apps from your browser or file manager). The first time, it asks for your
  Frontiers disc image in Android's own file picker, so copy the image to the device first; it keeps
  it for the next time.
- The touch screen's controller is there from the start, as in the browser on a phone. A controller
  (Bluetooth or USB) or a keyboard plays as on a computer, and Back is Escape.
- Tapping one of the game's text boxes (your account, the server's address, a character's name, a
  chat line) brings up the device's keyboard, and the game moves into the space above it while you
  type, so you see what you write.
- Going to the home screen keeps the game going for a while: come back and you are where you were.
- The app does not update itself: install the newer APK from this page over the old one, and your
  settings stay. Not on Android yet: a controller's rumble, and the 2002 art (F9).

## 2026-11-02 · A smaller touch pad, no blank screen while typing, fishing's message
- Options > Extended > Controller has a new row, Touch Pad Size: the touch screen's controller
  from 100 (its full size, the largest that fits the screen) down to 50, in steps of 5. Smaller,
  each half of the pad shrinks toward its own bottom corner, and Select and Start toward the middle,
  so more of the game shows round it.
- In the browser on a phone or a tablet, the screen no longer blanks out when the keyboard comes up
  for a chat line or goes away after it, nor when you turn the device or the page goes full screen.
- Fishing: the small dark square under the fishing messages ("You cast your line" and the rest) is
  gone, as it never showed on a PlayStation 2.

## 2026-10-31 · No more problem or crash reports
- F3 and the HUD's Report a bug button are gone, as is the offer to send a crash report at the next
  start after the game stopped: the game sends no reports of any kind. In the browser the gate's
  Report a problem and its link to the reports board are gone too.
- The game still keeps its log on your computer: eqoa.log in the game's settings folder, the run
  before as eqoa.prev.log. When the game stops on a fault, the log says why.

## 2026-10-31 · Every branch of your Master Class can be bought
- Class Mastery: some Master Classes branch into two abilities after the first rank (Spellbinder's
  Binding Grasp and Mana Flow). Having bought one, the other is no longer refused with "You cannot
  possess more than one Master Class at a time."
- On eqoaclassic, once you owned a Master Class, none of its abilities could be bought after the
  Class Mastery window had been closed and opened again (the same refusal); they can now. Buying a
  second, different Master Class is still refused.

## 2026-10-31 · Glowing weapons shine again
- Many weapons and things in the world give off a light of their own on a PS2, and this client
  had been drawing them only as bright as the light around them. The Sword of Virtue's blade,
  guard and pommel now gleam white and pale blue by day and night, as on a PlayStation 2, and so
  do Virtus Ferrum and the blades made like it (the Nitefyre Scimitar, Northwind Longsword, Blood
  Iron Longblade), and a great many glowing staves, wands, totems, rapiers and axes. The same goes
  for the glowing crystals, lamps and creatures' eyes the world has.
- Character select now shows what the PlayStation 2 shows there: the white flames on the stage's
  braziers, their sparks rising, and your weapon's glow (an epic weapon's column of light) in your
  character's hand.
- A problem report (F3) now says, for each creature round you, whether the server lets it be
  targeted: press F3 next to something you cannot target.

## 2026-10-31 · Sound in a crowd, Sandstorm's boon, walking while casting
- Sounds no longer cut out in a big fight or a crowd. When many creatures swing, get hit or cast
  at once, the game used to lose the sounds that did not fit its one message a frame to the sound
  chip (a PS2 does too, about one frame in ten of a 24-creature fight), and with more than the
  game's 24 creatures shown (NPC/PC Display) many more of them. Every sound now plays.
- As on a PS2, creatures behind the camera or a wall no longer make their swing, hit and step
  sounds, and with more than 24 creatures shown only the 24 nearest you are heard.
- When Sandstorm's boon throws everyone in Freeport into the sky, you fall at once now, as on a
  PS2. The game kept a second's loading screen for any trip of 200 or more, which held you in the
  air with nothing sent: to everyone else you hung there, and players thrown up after you landed
  on you and stood in the air. A long trip that loads a new place (Return Home, a coach to another
  town) still keeps its second.
- While casting you walk for exactly as long as on a PS2 (a hair longer than before). A problem
  report (F3) now says what is keeping you walking and what your character's legs were doing, so
  send one right when you are stuck walking or your steps stutter.

## 2026-10-30 · Your character there from your first moment in the world
- Now and then you could come into the world without your own character drawn, and with an empty compass and bars, until something about you changed. The server's first word about you had come a moment before the world itself and was thrown away; it is kept now.

## 2026-10-30 · Ground fog as the PlayStation 2 shows it
- The low fog that lies over Freeport's streets, Neriak and the rest of the world is now drawn the
  way the PlayStation 2's picture kept it. The console's screen held only 32 shades of each colour,
  so each thin layer of fog either changed the picture by a whole shade or not at all: its fog came
  out as a long, low, dimmer band, where ours was a brighter cloud. That look, with the console's
  fine dither grain in it, is the new default.
- If you prefer the brighter fog, Options, Extended, World has a new row, Ground Fog: PS2 (the
  default) or Exact. Shadows moved from World to Characters to make room for it.

## 2026-10-30 · Master Class ranks can be bought again
- Class Mastery: buying the next rank of the Master Class you already have no longer stops with
  "You cannot possess more than one Master Class at a time." Only a second, different Master Class
  is refused.

## 2026-10-30 · Spell effects' missing shapes
- Some spell effects carry moving shapes as well as sparkles, and those were never drawn. The
  warrior's Furious, Frenzied and Desperate Defense now raise their column of yellow-green flame
  round the warrior, and another effect its two green glowing forms sweeping round the caster, as
  on a PlayStation 2.

## 2026-10-30 · Spell lightning lit like the world
- The lightning of spells (a wizard's or a druid's bolt and the like) is lit by the day, the night
  and your lantern as the PlayStation 2 lights it: in daylight a blue or green bolt comes out paler
  and brighter, at night it keeps more of its own colour. It had been drawn in its bare colour at
  every hour. The trails of weapons stay as they were: the game draws them unlit.

## 2026-10-30 · Glowing weapons and spell effects as bright as on a PS2
- Every particle was drawn at half the strength the PlayStation 2 draws it with: the glow round an
  epic or magic weapon such as the Sword of Virtue, the sparkles, clouds and flashes of every spell,
  the fires, smoke and sparkles placed in the world, rain and snow, the stars, and the lightning of
  spells and the trails of weapons. They now light up as much as on a console: a glowing blade
  glows again.
- How it was found: the same character with the Sword of Virtue in hand and Minor Healing cast on
  it, side by side with the decompiled game's own drawing. The glow came out at about two thirds of
  the game's, and now matches it; the world's models, the sky's dome, water and the HUD are drawn
  as before.

## 2026-10-30 · SOE's final patch features, recovered from the Corsten and Lostologist save states!
- The game kept changing after the Frontiers disc, and its final client went with SOE's servers: no
  disc ever had it. Two save states made in the live game, Corsten's and Lostologist's, still held it
  in the console's memory, and the features below were read out of it and rebuilt, as they worked
  when the servers closed. They need a server that runs them (eqoaclassic does); on Sandstorm and
  other servers nothing changes, except for the lines that say "on every server".
- **How close is it to SOE's?** Everything you see is SOE's own: each window's layout, text, keys and
  rules, and every message the client sends and reads, were read instruction by instruction out of
  that final client and made to do the same. (The rest of this client is checked bit for bit
  against the decompiled Frontiers disc; the final client has no decompilation, so these were read
  straight from its machine code.) What SOE's servers did with those messages went with the
  servers: where the client does not say, eqoaclassic does what we chose to fit what the client
  expects. That covers who a group search finds, how a stack's items are shared among several
  bidders, the words a reset says, and duel hits on players being halved.
- **Duels.** The start menu's Community has a Duel entry: Duel > Invite challenges your target, Duel >
  Surrender gives up the duel you are in. Quitting during a duel gives it up too.
- A challenge opens the game's own duel window: who challenges you, their guild, race, class and
  level, with ✕ to accept and △ to decline (Enter or E, and Escape, on the keyboard). It counts its
  last ten seconds down; an invitation that comes while another window is up waits until you are
  back in play.
- Accepted, both of you get the wager window, the trade window's twin: put tunar on the duel with
  ✕ on the Tunar box (no more than you carry), □ (Q) to accept, △ to take an accept back or to call
  the duel off, Space for a chat line. When both accept the duel begins, and the winner takes both
  stakes. Only tunar can be wagered: an item put in is refused with a line.
- In a duel you can attack your opponent and their pet as you would a creature, and Tab and R1
  cycle to their pet. An orange mark on the compass points to where the duel began.
- **Looking for a group.** The Group menu has Search, Register and Unregister. Register (or typing
  `/lfg on`) puts you on the list of players looking for a group, Unregister (`/lfg off`) takes you
  off it. Search opens the game's window for it: pick any races and classes and a level range (type
  -1 for the levels you could hunt with), Search, and the players found are listed with their race,
  class and level, sorted by any column with R1 or Tab and L1. ✕ on a player invites them to your
  group or sends them a tell; □ shows the window's help. A group's leader looks for players to fill
  it, a player alone for a group.
- **Auctions of stacks.** You can put up a whole stack, or part of one: the form asks how many, and
  the prices are each item's. Bidding asks how many you want, "Buy all now" buys the whole stack at
  its price each, and the bidders' list shows how many each one asked for. Items with Lore can be put
  up too. The Ogre and the Alchemist show in the auction's pickers and in an item's races and
  classes only when the server has them.
- **Mail to several players.** A letter can go to several players at once: write their names in To
  with commas or semicolons between them. If a name is nobody's, the letter is not sent and the box
  names who was not found, so you can put it right. If someone's mailbox is full, or they have too
  many unread letters from you, the chat says so in red, naming them, and everyone else gets it.
- A letter you read shows who it went to, under From, and its options have Reply All between Reply
  and Delete: it answers the sender and everyone else it went to.
- **Dyed armour.** Armour, robes and helms can be any colour, sent the way the game did it at its
  end (on eqoaclassic a game master sets them). Each dyed piece is drawn in its colour on your
  character, on everyone near you and on the character select screen. Armour that is not dyed looks
  exactly as before.
- **On every server, Sandstorm included:** buying a second Master Class while you have one is
  refused at once, with "You cannot possess more than one Master Class at a time." in red, where it
  used to only warn and then let you ask the server anyway.
- On every server: destroying an item asks 'Destroy item "<its name>"?', so you can see which one
  you picked, and selling one to a merchant asks 'Sell item "<its name>"?' for everything but loot.
  Both start on No.
- On every server: a petition the staff have closed no longer says only "System is temporarily
  unavailable. (Code: -8)": the game's explanation follows, that the petition may have been
  addressed (the box has room for its start, as the game's had). In your inbox, up from the first
  letter goes round to the last, and down from the last to the first. The Group menu's Show Chat
  and Hide Chat now tell the server (they did nothing).
- For GMs, on every server: the game's /rst_mastr, /rst_both, /rem_abil, /banktunar and /servdown
  are typed in the chat, and work while the server has your GM commands on.
- **Also in this build:** your own character is now drawn at the size the server gives it, as
  everyone else is: on eqoaclassic a gnome is a little smaller than before (9 tenths). How you walk
  and what you bump into are as before.

## 2026-10-29 · The who list in CM order on Sandstorm, as on a PS2
- On Sandstorm the who list now shows players in the server's own order, highest combat masteries
  first, whenever you open it again or press Update and nobody has come or gone since: Sandstorm's
  patch keeps the server's order then, as it does on a PS2. The first time you open it, and after
  L1 or R1 (left or right), it is sorted by the column as before.

## 2026-10-29 · One question for all your crash reports, and freezes written down
- If the game stopped more than once since you last sent its crash reports, the next start now
  asks about all of them at once and says how many there are. Send sends them all, Delete deletes
  them all, and Not now keeps them for later. Until now it asked about one at a time, the newest,
  so the same question came back at every start.
- The music is now read from the disc ahead of playing it, as the browser version does. A slow
  disc (a drive waking up, or a busy one) now only makes the music start a little late, where it
  could hold the whole game up for as long as the read took.
- When the game freezes for a second or more, it now writes down how long, and what it was doing,
  in its log. A problem report (F3) sent afterwards carries that, and when you play two windows it
  carries the other window's too, so a report of freezes says what held them up.

## 2026-10-29 · Space loots a corpse as well as opening it
- In a corpse's window Space now does what the controller's ✕ does there, the button that opened
  it: on LEAVE ALL it closes the window, on an item it takes that item, and on TAKE ALL it takes
  everything. It used to do nothing there, so you had to switch to E or Enter.

## 2026-10-29 · A steady camera while you strafe with the mouse held
- Running while holding a mouse button (the right one, or the left one while you move) and
  stepping sideways with A or D no longer makes the camera shake from side to side, which blurred
  the picture at a high frame rate. It now follows you as smoothly as with the keys alone.
- The same goes for dragging the camera round with a finger on a touch screen while you run.

## 2026-10-29 · Playing over a phone's hotspot
- The game connects on networks that give servers IPv6 addresses, as many phone hotspots do: it
  now takes a server's IPv4 address when there is one, and on a network with IPv6 alone it goes
  through the network's own way to IPv4. Before, the server list stopped there with "An address
  incompatible with the requested protocol was used".

## 2026-10-29 · Longer passwords at the login
- The Station login, the new-account page and the change-password page now take a password of up
  to 31 characters, the most the login can send. They stopped at 15, so a longer password made on
  a server's website could not be typed in.

## 2026-10-29 · A trade no longer opens over your inventory
- When the other player accepts your trade while you have your inventory (or any other part of the
  Start menu) open, the menu now closes and the trade window opens on its own, as on the PS2. Both
  used to stay up, taking your button presses, so an item you picked could look as if it had been
  copied.
- Opening the Start menu after that shows the main menu, not the inventory it was closed over.

## 2026-10-29 · The aiming crosshair reaches no farther than a tap
- Holding R1 or L1 (Tab or ` on the keyboard) to aim with the crosshair now targets only what a
  tap of the same button could: creatures within 200 of the camera. It used to take whatever lay
  nearest the crosshair however far off, even creatures too far away to be drawn, and with a long
  NPC/PC View Distance out to 1,000.

## 2026-10-29 · A creature's second body
- A server can send a creature or a player with a second body to show in place of its own (the
  live game did, for its upgraded models). This client now shows that body, and bumps into it,
  as the PS2 does: until now it always showed the first one.

## 2026-10-29 · Invisible walls stop you again
- An invisible wall that the server puts up as a creature now stops you, as on the PS2.
  Sandstorm uses them to keep players out of a place until a quest lets them through, and this
  client let you walk straight through them: it made a creature solid only once it had a body
  to draw, and these have none.

## 2026-10-28 · No stand-in when you enter the world
- When you enter the world or change worlds, a human man no longer stands in for your character
  for a moment before your own appears: the game draws you once the server has described you, as
  the PS2 does.

## 2026-10-28 · Steering with the mouse runs as the stick does; coaches with the menu open
- Steering with the mouse (the right button held, or either button in first person) keeps your
  run as it is with the keys or a controller again. Since yesterday's update, a mouse that moved
  held your speed at what it was when the mouse began to move: from a stand you only crept
  forward until the mouse stopped, a run you let go of slid on, and on slopes your character sank
  into the ground going up and floated over it going down.
- A coach, a boat or a port that moves you while the start menu or the inventory is open now
  closes it first and gives you your own camera back, as on the PS2. The camera had stayed in the
  menu's close-up of your character, and after opening and closing the menu again it sat at one
  angle round you instead of following behind you. Dying with the menu open is fixed the same way.
- Problem reports (F3) now say how the connection to the server is doing, so a report from a
  world where no one else shows up tells whether the server still hears you.

## 2026-10-28 · GMs see their server's names for the TRIGGER entries
- For GMs only, on a server that names its triggers (eqoaclassic): TEST, Programming, TRIGGER
  now shows each trigger's name where the PS2 shows eight lines all called TRIGGER. A trigger
  that is switched off is greyed out, and one that is going on now has a * after its name. A
  line with no trigger still says TRIGGER. Picking a line fires it as before. Every other
  player, and every other server, sees no change.

## 2026-10-28 · Goto Zone lands on the ground; fishing on eqoaclassic
- The GM's Goto Zone (TEST, Navigation) now sets you down on the ground of the square you pick,
  as the PS2 does. It left you under the ground there, and you fell.
- Fishing now works on eqoaclassic: buy a Fishing Pole from a Pole Merchant and a can of worms
  from the Tacklemaster beside him (Freeport's docks, the Oasis, Hazinak and more), wear both,
  face the water and say /fish (or use a weapon set that holds the pole).

## 2026-10-28 · Shadows under the characters, fishing, and the GM's TEST menu
- Every character near the camera now has a shadow under its feet, as on the PS2: a soft dark
  patch on the ground, and on docks, counters and steps, fading where the ground falls away.
  Inside a building, as on the PS2, only the characters you could see through its doorways
  and openings cast one. Options, Extended, World, Shadows turns them off.
- Spell lightning and weapon trails are drawn over the spells' sparks and the shadows, and
  under the rain and snow, in the PS2's order (they were drawn under the sparks).
- Fishing works as on the PS2 on a server that offers it (none of ours does yet): the Fishing
  window, its power meter (✕ or the left stick to cast and throw), the catch list (Take All,
  one item, Leave All), its messages, and the splash on the water.
- On a server that gives your account the game's GM channel, the start menu shows TEST first,
  as a GM's PS2 did, with its Buff PC and Goto windows. Every other player sees no change.
- A full buddy list on the other side now says so ("Add buddy failed: Your buddy's buddy list
  is full."), and the game answers the server's connection check and its other housekeeping
  as a PS2 does. Your position goes to the server at the PS2's own rate (every 101.7 ms; it
  was 100).

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
