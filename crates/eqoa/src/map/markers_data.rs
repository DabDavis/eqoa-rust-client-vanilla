// Code written by tools/gen_map_markers.py; DO NOT EDIT: change the tool and run it again.
//
// The XP camps are EQOA-Data's (github.com/Jadiction/EQOA-Data, MIT: its map's camps with a
// note, its XP-camp guide; their rare creatures and its biome loot tables' items), each at the
// place go-eqoa-server's tools/gen_camp_teles.py gives it; the coaches are each coachman's
// spawn, the town go-eqoa-server's coach menus name for it and the towns EQOA-Data's coach map
// has its coach run to; the towns are EQOA-Data's, Tunaria's where go-eqoa-server's coach menus
// put them, and the routes its coach map's.

use super::markers::{Camp, Coach, Route, Side, Town};

pub const CAMPS: [Camp; 49] = [
    Camp { name: "Deathfist Citadel (DFC)", place: "Deathfist Citadel", world: 0, at: [21080.5, 19078.3], levels: (29, 34), note: "Deathfist Citadel Zone. One of the best XP camps in the low 30s. Watch for trains and deep pulls.", rare: &[], own: &[], table: "Desert", drops: &["Cleansing Belt", "Puma Hide Boots", "Pyroweave Tunic", "Seeking Rapier"] },
    Camp { name: "Deathfist Slayers & Reavers", place: "Al Farak Ruins' Deathfist slayers and reavers", world: 0, at: [22377.3, 21305.1], levels: (30, 33), note: "Al Farak Ruins (S/SE). Spread-out pulls make this a safer alternative to DFC.", rare: &[], own: &[], table: "Desert", drops: &["Cyclone Medallion", "Grimknuckle", "Poet's Earring", "Whistling Sabre"] },
    Camp { name: "Hexbones & Shrieks", place: "Zentar's Keep's hexbones and shrieks", world: 0, at: [10306.6, 8377.7], levels: (39, 44), note: "Zentar's Keep. Caster-heavy mobs. Snow griffons may interfere with pulls.", rare: &[], own: &[], table: "Forest", drops: &["Clovermist Gloves", "Old Mage's Crook", "Pinelash Belt", "Shield of Flowers"] },
    Camp { name: "Nasehir", place: "Northern Ro's Nasehir, the first camp", world: 0, at: [24346.3, 16213.8], levels: (10, 14), note: "Northern Ro (NW/W). Two separate camps near the coast. Consistent pulls and solid group XP.", rare: &[], own: &[], table: "Desert", drops: &["Inspiring Band", "Pulsing Bauble", "Starblaze Choker", "Collar of Conflict"] },
    Camp { name: "Nasehir", place: "Northern Ro's Nasehir, the second camp", world: 0, at: [24539.3, 17137.9], levels: (10, 14), note: "Northern Ro (NW/W). Two separate camps near the coast. Consistent pulls and solid group XP.", rare: &[], own: &[], table: "Desert", drops: &["Inspiring Band", "Pulsing Bauble", "Starblaze Choker", "Collar of Conflict"] },
    Camp { name: "Hatebone Orcs", place: "Halas' Hatebone orcs", world: 0, at: [19957.6, 7422.3], levels: (1, 6), note: "Halas (Outskirts). A solid beginner camp with tightly packed orcs. Good for melee-heavy groups learning to pull.", rare: &[], own: &[], table: "Forest", drops: &["Sword of Faith"] },
    Camp { name: "Hatebone Orcs", place: "Halas' Hatebone orcs, the second camp", world: 0, at: [19946.4, 7943.2], levels: (1, 6), note: "Halas (Outskirts). A solid beginner camp with tightly packed orcs. Good for melee-heavy groups learning to pull.", rare: &[], own: &[], table: "Forest", drops: &["Sword of Faith"] },
    Camp { name: "Hatebone Orc Cave", place: "Halas' Hatebone orc cave", world: 0, at: [19840.2, 7727.4], levels: (1, 6), note: "Halas (Outskirts). A solid beginner camp with tightly packed orcs. Good for melee-heavy groups learning to pull.", rare: &[], own: &[], table: "Forest", drops: &["Sword of Faith"] },
    Camp { name: "Undead", place: "Northern Ro's undead", world: 0, at: [24123.5, 16547.0], levels: (17, 20), note: "Northern Ro (NW/SW). Two undead camps offering reliable experience. Clerics excel here.", rare: &[], own: &[], table: "Desert", drops: &["Joldan's Trinket", "Lifeboon Ring", "Ring of Combustion", "Ring of Grounding"] },
    Camp { name: "Undead", place: "Northern Ro's undead, the second camp", world: 0, at: [24329.7, 17688.8], levels: (17, 20), note: "Northern Ro (NW/SW). Two undead camps offering reliable experience. Clerics excel here.", rare: &[], own: &[], table: "Desert", drops: &["Joldan's Trinket", "Lifeboon Ring", "Ring of Combustion", "Ring of Grounding"] },
    Camp { name: "Gharial", place: "the gharial island south of Moradhim", world: 0, at: [23325.2, 20427.6], levels: (35, 38), note: "South of MTG (Ocean). Island clearing camp. Watch for wanderers crossing into camp.", rare: &[], own: &[], table: "Desert", drops: &["Glinting Chestguard", "Offsetting Earring", "Torkark's Tower", "Earring of Vigor"] },
    Camp { name: "Tendora-Buki", place: "Tendora-Buki, West Toxulia's kerran village", world: 2, at: [5649.0, 4936.1], levels: (44, 53), note: "West Toxulia (SW). High-risk, high-reward camp. Hut clearing becomes viable at higher levels.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Kappa", place: "Slythis Dwell's kappa", world: 2, at: [4907.8, 2298.7], levels: (50, 55), note: "Slythis Dwell. Often underused camp with strong XP potential.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Kappa Warrior", place: "Slythis Dwell's kappa warriors", world: 2, at: [4886.8, 3786.7], levels: (50, 55), note: "Slythis Dwell. Often underused camp with strong XP potential.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Kappa Elites", place: "Slythis Dwell's kappa elites", world: 2, at: [4877.1, 2346.1], levels: (53, 58), note: "Slythis Dwell (Castle Balcony). Top-floor elite camp. Requires strong tank and healer coordination.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Kappa Drudge", place: "the East Plateau's kappa drudges", world: 2, at: [6198.5, 2776.5], levels: (36, 44), note: "Odous / East Plateau. Slower spawn timers but strong experience. North and west of Arcadin.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Phantoms", place: "the East Plateau's phantoms", world: 2, at: [7057.9, 3366.3], levels: (50, 51), note: "East Plateau (NW of Arcadin). Undead-style camp with valuable scroll drops.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Gargoyle", place: "the South Barren Coast's gargoyles", world: 2, at: [10209.5, 8966.7], levels: (50, 53), note: "South Barren Coast. Strong melee mobs with quest-related drops.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Harpy", place: "Erud's Crossing's harpies", world: 2, at: [11764.5, 2293.8], levels: (55, 60), note: "Erud's Crossing. Efficient XP with manageable damage output.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Moots", place: "Erud's Crossing's moots", world: 2, at: [10789.3, 3335.1], levels: (57, 60), note: "Erud's Crossing (South of Harpies). Fire-based spellcasters. Fire resistance recommended.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Gnolls", place: "Blackburrow's gnolls", world: 0, at: [12913.1, 12611.0], levels: (1, 8), note: "Blackburrow Area. Classic early gnoll camp. Watch for adds inside the cave system.", rare: &[], own: &[], table: "Forest", drops: &["Sword of Faith"] },
    Camp { name: "Bloodfoots", place: "Mt. Hatespike's Bloodfoots", world: 0, at: [12371.0, 11155.9], levels: (26, 32), note: "Mt. Hatespike (W). Good humanoid camp with manageable aggro ranges. Casters may drop useful shields.", rare: &[], own: &[], table: "Arctic", drops: &["Bowmaster's Greaves", "Dilian's Compendium", "Exacting Boots", "Gladenguard"] },
    Camp { name: "Coldpaw Gnolls", place: "Klik'Anon's Coldpaw gnolls", world: 0, at: [22462.1, 6734.0], levels: (4, 7), note: "Klik'Anon (W). Low faction impact compared to other gnolls. Good camp layout for small groups.", rare: &[], own: &[], table: "Mountain", drops: &["Wizard's Staff"] },
    Camp { name: "Hill Giants", place: "Keliner's hill giants", world: 0, at: [13711.6, 24281.0], levels: (44, 49), note: "Keliner (NE). Classic giant camp with high HP targets and steady XP.", rare: &[], own: &[], table: "Plains", drops: &["Bloodfyre Morning Star", "Bloodsting Kris", "Centaurian War Staff", "Dreadswamp Offering"] },
    Camp { name: "Serpent Guardians", place: "the Bandit Hills' serpent guardians", world: 0, at: [15644.7, 15129.5], levels: (24, 29), note: "Bandit Hills (E). Confined cave camp. Bring crowd control to manage close spawns.", rare: &[], own: &[], table: "Forest", drops: &["Twisted Tree Branch", "Bamboozler", "Green Ember Axe", "Pixie Stabber"] },
    Camp { name: "Cutthroats & Bandits", place: "Mulin's Reach's cutthroats and bandits", world: 0, at: [23685.0, 13593.2], levels: (14, 18), note: "Mulin's Reach (SE). Straightforward humanoid camp. Good coin drops and steady experience.", rare: &[], own: &[], table: "Forest", drops: &["Stumpy's Stick", "Black Lace Sash", "Dirk of Timbers", "Grassy Entwined Twig"] },
    Camp { name: "Cutthroats & Bandits", place: "Mulin's Reach's cutthroats and bandits, the second camp", world: 0, at: [22377.4, 13533.7], levels: (21, 24), note: "Mulin's Reach (SE). Straightforward humanoid camp. Good coin drops and steady experience.", rare: &[], own: &[], table: "Forest", drops: &["Entwined Belt", "Forest Charm", "Forest Mist Staff", "Bamboo Basher"] },
    Camp { name: "Orc Plainstriders", place: "Mulin's Reach's orc plainstriders", world: 0, at: [23304.3, 13235.3], levels: (17, 21), note: "Mulin's Reach (N/NE). Popular leveling path. Open terrain makes pulling simple.", rare: &[], own: &[], table: "Forest", drops: &["Elfbone Spear", "Keen Elven Dagger", "Mace of the Elddar", "Old Elven War Shield"] },
    Camp { name: "Orc Plainsraiders", place: "Mulin's Reach's orc plainsraiders", world: 0, at: [23208.4, 12257.3], levels: (17, 21), note: "Mulin's Reach (N/NE). Popular leveling path. Open terrain makes pulling simple.", rare: &["Chief Klumox"], own: &[], table: "Forest", drops: &["Elfbone Spear", "Keen Elven Dagger", "Mace of the Elddar", "Old Elven War Shield"] },
    Camp { name: "Cracktooth Orcs", place: "Bobble-by-Water's Cracktooth orcs", world: 0, at: [24997.8, 12411.4], levels: (19, 24), note: "Bobble-By-Water. Excellent faction gains and consistent pulls. Great training camp for tanks.", rare: &[], own: &[], table: "Forest", drops: &["Entwined Belt", "Forest Charm", "Forest Mist Staff", "Bamboo Basher"] },
    Camp { name: "Druidic Cadavers", place: "the druidic cadavers west of Qeynos", world: 0, at: [4309.2, 15805.2], levels: (1, 10), note: "Qeynos (NW). Reliable undead camp for early levels. Safe pulls with minimal wandering mobs.", rare: &[], own: &[], table: "Forest", drops: &["Deer Horn", "Bracers of Bark", "Brawler's Belt", "Pine Tree Branch"] },
    Camp { name: "Slesher Gnoll Camp", place: "Surefall Glade's Slesher gnolls", world: 0, at: [9621.5, 13734.5], levels: (10, 14), note: "Surefall Glade. Compact gnoll camp with manageable pulls. Works well for balanced groups.", rare: &[], own: &[], table: "Forest", drops: &["Cured Silk Belt", "Tunic of Dew", "Gold Leaf Necklace", "Jade Scarb Earring"] },
    Camp { name: "Aviak Outpost", place: "the aviak outpost near Halas", world: 0, at: [14549.2, 5296.2], levels: (1, 1), note: "Near Halas. Outdoor camp with roaming aviaks. Good experience but requires awareness.", rare: &["Oyric Winterwalk"], own: &[], table: "Arctic", drops: &[] },
    Camp { name: "Orc Tunnel", place: "the Druk's orc tunnel", world: 0, at: [18959.4, 9031.0], levels: (14, 18), note: "The Druk (C). High-density orc camp. Be cautious of trains from deeper inside.", rare: &[], own: &[], table: "Forest", drops: &["Stumpy's Stick", "Black Lace Sash", "Dirk of Timbers", "Grassy Entwined Twig"] },
    Camp { name: "Gnoll Fishermen", place: "the gnoll fishermen", world: 0, at: [13445.3, 13030.1], levels: (20, 26), note: "Darkmoor. Riverbank gnolls with moderate social aggro. Solid mid-20s camp.", rare: &[], own: &[], table: "Mountain", drops: &["Dawnfyre Stone Blade", "Goblin Fang Dagger", "Russet Clay Medallion", "Shaved Rock"] },
    Camp { name: "Scarecrows", place: "Alseop's Wall's scarecrows", world: 0, at: [12291.4, 17005.3], levels: (21, 31), note: "Alseop's Wall (W). Reliable undead-style mobs. Safe rotations for organized groups.", rare: &[], own: &[], table: "Forest", drops: &["Meadow Thicket", "Ruby Embossed Staff", "Singing Saber", "Underbrushes Mace"] },
    Camp { name: "Goblin Citadel", place: "the Goblin Citadel near Runneye", world: 0, at: [16250.8, 12219.3], levels: (30, 36), note: "Runneye Area. Dense goblin camp with solid experience. Crowd control recommended.", rare: &[], own: &[], table: "Mountain", drops: &["Crude Stone Amulet", "Crude Stone Mace", "Crude Stone Ring", "Gusty Leggings"] },
    Camp { name: "Pirates of Klick", place: "the Klik'Anon coast's pirates", world: 0, at: [25454.5, 6579.2], levels: (30, 38), note: "Klick'Anon Coast. Good coin and experience. Outdoor pulls with roaming patrols.", rare: &[], own: &[], table: "Forest", drops: &["Dagger of Destruction", "Mud Coated Staff", "Oak Inlaid Dagger", "Song of the Melospiza"] },
    Camp { name: "Cyclops Fortress", place: "the cyclops", world: 0, at: [11445.0, 21368.4], levels: (40, 46), note: "Desert Region. High health mobs with strong melee damage. Good for experienced groups.", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Siloths", place: "the Salt Mine's siloths", world: 0, at: [14358.4, 18446.7], levels: (45, 50), note: "Salt Mine. Dungeon-style camp. Valuable ring drops and strong experience.", rare: &[], own: &[], table: "Mountain", drops: &["Blood Iron Breastplate", "Bloodfyre Shard", "Darkfyre Knuckles", "Darkstone Tunic"] },
    Camp { name: "Ice Giant Refugees", place: "the ice giant refugees near Northgate", world: 0, at: [4861.5, 4617.2], levels: (46, 50), note: "Near Northgate. Multiple camps available. Expect heavy melee damage.", rare: &["Lady Vox (raid)"], own: &[], table: "Arctic", drops: &["Boneking's Crown", "Buzzing Bee", "cowl of reasoning", "Dilian's Wonder"] },
    Camp { name: "Ghosts", place: "the wandering ghosts", world: 0, at: [15548.9, 20787.5], levels: (58, 62), note: "Various High-Level Zones. Undead endgame camp. Works well with cleric-heavy groups.", rare: &[], own: &[], table: "Desert", drops: &[] },
    Camp { name: "Pariah of Xragg", place: "the Elephant Graveyard's children of Xrag", world: 0, at: [15000.4, 27406.8], levels: (60, 62), note: "Elephant Graveyard. High-end named camp. Valuable spell drops and crafting materials.", rare: &["Overlord Xrag"], own: &[], table: "Swamp", drops: &[] },
    Camp { name: "Tomb Rats", place: "the tomb rats", world: 0, at: [17767.0, 8183.4], levels: (1, 9), note: "Undead", rare: &[], own: &[], table: "", drops: &[] },
    Camp { name: "Icy wind & freezing gusts", place: "the freezing gusts", world: 0, at: [15734.1, 6740.6], levels: (13, 15), note: "Air & water elementalings", rare: &[], own: &[], table: "Mountain", drops: &["Miner's Hat", "Crude Stone Scimitar", "Fists of Stone", "Mountaineer Boots"] },
    Camp { name: "Goblins", place: "Guardian Forest's goblins", world: 0, at: [10182.1, 6973.6], levels: (56, 60), note: "Guardian Forest (W). Easy pulls with steady spawn cycles.", rare: &[], own: &[], table: "Arctic", drops: &[] },
    Camp { name: "Freezeblood Goblins", place: "Everfrost's Freezeblood goblins", world: 0, at: [13435.0, 6587.1], levels: (10, 15), note: "Everfrost. Undead camp with steady respawns. Casters should watch for wandering adds.", rare: &["Frostbite"], own: &[], table: "Arctic", drops: &["Bloodiron Belt", "Bracers of Inspiration", "Evergreen Staff", "Frosty Coil"] },
    Camp { name: "Saltwater Crocodiles", place: "Bobble-by-Water's saltwater crocodiles", world: 0, at: [25751.5, 11072.0], levels: (18, 20), note: "Bobble-By-Water (E). Island-based camp with social aggro. Pull carefully to avoid chain fights.", rare: &["Alleza"], own: &[], table: "Forest", drops: &["Feather Earring", "Handchopper", "Leafy Tunic", "Tree Carver"] },
    Camp { name: "Rustel Minotaur", place: "the Gerntar Mines' Rustel minotaurs", world: 0, at: [13002.0, 26869.2], levels: (29, 38), note: "Gerntar Mines (C). High mob density inside caves. Strong XP but can overwhelm unprepared groups.", rare: &[], own: &[], table: "Plains", drops: &["Plated Belt of Nife", "Three Pronged Spear", "Holdfast", "Marauder Gauntlets"] },
];

pub const COACHES: [Coach; 31] = [
    Coach { coachman: "Coachman Billfer", town: "Qeynos", world: 0, at: [4813.4, 17143.5], to: &["Wyndhaven", "Forkwatch", "Highbourne", "Surefall Glade"] },
    Coach { coachman: "Coachman Boblon", town: "Bobble-by-Water", world: 0, at: [24640.8, 11864.5], to: &["Rivervale", "Klik'Anon", "Freeport"] },
    Coach { coachman: "Coachman Brody", town: "Castle Lightwolf", world: 0, at: [9281.4, 6494.6], to: &["Halas", "Murnf"] },
    Coach { coachman: "Coachman Brumley", town: "Wyndhaven", world: 0, at: [4710.5, 13202.0], to: &["Surefall Glade", "Murnf", "Qeynos", "Honjour", "Zentar's Keep"] },
    Coach { coachman: "Coachman D'Larik", town: "Fort Seriak", world: 0, at: [21780.6, 8388.4], to: &["Neriak", "Highpass Hold", "Mt. Hatespike"] },
    Coach { coachman: "Coachman Dapkus", town: "Blackwater", world: 0, at: [15151.6, 22665.0], to: &["Gerntar Mines", "Oasis", "Forkwatch"] },
    Coach { coachman: "Coachman Dilga", town: "Oggok", world: 1, at: [8961.9, 7213.7], to: &["Dark Solace", "Kerplunk Outpost"] },
    Coach { coachman: "Coachman Doba", town: "Moradhim", world: 0, at: [15351.6, 9267.1], to: &["Halas", "Rivervale", "Fayspire", "Darvar Manor"] },
    Coach { coachman: "Coachman Droogra", town: "Hazinak (Evil)", world: 0, at: [25128.9, 27156.3], to: &["Oasis", "Grobb"] },
    Coach { coachman: "Coachman Dudley", town: "Halas", world: 0, at: [12971.1, 4683.8], to: &["Moradhim", "Castle Lightwolf"] },
    Coach { coachman: "Coachman Farn", town: "Muniel's Tea Garden", world: 0, at: [23369.9, 19636.2], to: &["Oasis", "Freeport"] },
    Coach { coachman: "Coachman Frender", town: "Surefall Glade", world: 0, at: [9136.3, 12737.3], to: &["Wyndhaven", "Darvar Manor", "Qeynos"] },
    Coach { coachman: "Coachman Galdah", town: "Highbourne", world: 0, at: [4858.6, 21458.7], to: &["Qeynos", "Forkwatch"] },
    Coach { coachman: "Coachman Huff", town: "Rivervale", world: 0, at: [18334.8, 11357.8], to: &["Bobble-by-Water", "Moradhim", "Highpass Hold"] },
    Coach { coachman: "Coachman Lothi", town: "Neriak", world: 0, at: [24925.4, 9429.4], to: &["Klik'Anon", "Fort Seriak", "Freeport"] },
    Coach { coachman: "Coachman Nioclas", town: "Dark Solace", world: 0, at: [12539.3, 21182.0], to: &["Kerplunk Outpost", "Oasis", "Highpass Hold", "Honjour"] },
    Coach { coachman: "Coachman Norial", town: "Honjour", world: 0, at: [9805.1, 14990.3], to: &["Mt. Hatespike", "Wyndhaven", "Dark Solace"] },
    Coach { coachman: "Coachman Opelim", town: "Mt. Hatespike", world: 0, at: [13758.4, 11398.1], to: &["Fort Seriak", "Honjour"] },
    Coach { coachman: "Coachman Quicksteed", town: "Highpass Hold", world: 0, at: [16780.8, 15356.2], to: &["Freeport", "Rivervale", "Dark Solace", "Darvar Manor", "Fort Seriak"] },
    Coach { coachman: "Coachman Rizkar", town: "Klik'Anon", world: 0, at: [23463.6, 6489.4], to: &["Fayspire", "Neriak", "Bobble-by-Water"] },
    Coach { coachman: "Coachman Ronks", town: "Freeport", world: 0, at: [25273.7, 15719.1], to: &["Neriak", "Bobble-by-Water", "Highpass Hold", "Muniel's Tea Garden"] },
    Coach { coachman: "Coachman Skeel", town: "Zentar's Keep", world: 0, at: [5500.7, 6453.6], to: &["Wyndhaven"] },
    Coach { coachman: "Coachman Thirtreel", town: "Fayspire", world: 0, at: [19482.3, 7043.9], to: &["Tethelin", "Klik'Anon", "Rivervale", "Moradhim"] },
    Coach { coachman: "Coachman Ungar", town: "Gerntar Mines", world: 0, at: [13027.1, 27501.6], to: &["Blackwater"] },
    Coach { coachman: "Coachman Zaris", town: "Tethelin", world: 0, at: [18425.7, 7494.7], to: &["Fayspire"] },
    Coach { coachman: "Coachman Zelik", town: "Hazinak (Good)", world: 0, at: [24875.4, 27606.8], to: &["Oasis"] },
    Coach { coachman: "Coachman Zuggug", town: "Grobb", world: 0, at: [25223.0, 31175.3], to: &["Hazinak (Evil)", "Kerplunk Outpost"] },
    Coach { coachman: "Coachwoman Dainei", town: "Murnf", world: 0, at: [8936.8, 10506.5], to: &["Castle Lightwolf", "Wyndhaven"] },
    Coach { coachman: "Coachwoman Darva", town: "Darvar Manor", world: 0, at: [13603.2, 14762.7], to: &["Moradhim", "Surefall Glade", "Forkwatch", "Highpass Hold"] },
    Coach { coachman: "Coachwoman Dernia", town: "Oasis", world: 0, at: [21118.3, 25173.3], to: &["Dark Solace", "Hazinak (Good)", "Hazinak (Evil)", "Blackwater", "Muniel's Tea Garden"] },
    Coach { coachman: "Coachwoman Holly", town: "Forkwatch", world: 0, at: [12527.8, 18760.7], to: &["Qeynos", "Blackwater", "Highbourne", "Darvar Manor"] },
];

/// The towns, the biggest first.
pub const TOWNS: [Town; 36] = [
    Town { name: "Qeynos", world: 0, at: [4829.0, 17154.0], side: Side::Good }, // 461 spawns about it
    Town { name: "Fayspire", world: 0, at: [19473.0, 7038.0], side: Side::Good }, // 426 spawns about it
    Town { name: "Freeport", world: 0, at: [25270.0, 15710.0], side: Side::Neutral }, // 375 spawns about it
    Town { name: "Neriak", world: 0, at: [24929.0, 9438.0], side: Side::Evil }, // 276 spawns about it
    Town { name: "Moradhim", world: 0, at: [15352.0, 9262.0], side: Side::Good }, // 246 spawns about it
    Town { name: "Arcadin", world: 2, at: [8751.3, 4137.1], side: Side::Good }, // 209 spawns about it
    Town { name: "Highbourne", world: 0, at: [4854.0, 21477.0], side: Side::Good }, // 195 spawns about it
    Town { name: "Halas", world: 0, at: [12963.0, 4689.0], side: Side::Good }, // 180 spawns about it
    Town { name: "Oggok", world: 1, at: [8961.9, 7213.7], side: Side::Evil }, // 165 spawns about it
    Town { name: "Highpass Hold", world: 0, at: [16773.0, 15363.0], side: Side::Neutral }, // 163 spawns about it
    Town { name: "Grobb", world: 0, at: [25257.0, 31226.0], side: Side::Evil }, // 135 spawns about it
    Town { name: "Klik'Anon", world: 0, at: [23460.0, 6488.0], side: Side::Neutral }, // 130 spawns about it
    Town { name: "Tethelin", world: 0, at: [18421.0, 7488.0], side: Side::Good }, // 119 spawns about it
    Town { name: "Rivervale", world: 0, at: [18339.0, 11350.0], side: Side::Good }, // 118 spawns about it
    Town { name: "Muniel's Tea Garden", world: 0, at: [23374.0, 19638.0], side: Side::Neutral }, // 114 spawns about it
    Town { name: "Bobble-by-Water", world: 0, at: [24634.0, 11861.0], side: Side::Good }, // 92 spawns about it
    Town { name: "Hazinak (Evil)", world: 0, at: [25130.0, 27159.0], side: Side::Evil }, // 92 spawns about it
    Town { name: "Zentar's Keep", world: 0, at: [5496.0, 6457.0], side: Side::Evil }, // 86 spawns about it
    Town { name: "Surefall Glade", world: 0, at: [9060.0, 12978.0], side: Side::Good }, // 84 spawns about it
    Town { name: "Stone of Morathals (Evil)", world: 2, at: [6847.5, 10980.8], side: Side::Evil }, // 82 spawns about it
    Town { name: "Hazinak (Good)", world: 0, at: [24885.0, 27594.0], side: Side::Good }, // 76 spawns about it
    Town { name: "Darvar Manor", world: 0, at: [13606.0, 14766.0], side: Side::Neutral }, // 69 spawns about it
    Town { name: "Honjour", world: 0, at: [9804.0, 14991.0], side: Side::Evil }, // 69 spawns about it
    Town { name: "East Plateau", world: 2, at: [8085.3, 2143.6], side: Side::Evil }, // 63 spawns about it
    Town { name: "Oasis", world: 0, at: [21120.0, 25167.0], side: Side::Neutral }, // 62 spawns about it
    Town { name: "Forkwatch", world: 0, at: [12538.0, 18774.0], side: Side::Good }, // 60 spawns about it
    Town { name: "Stone of Morathals (Good)", world: 2, at: [7235.8, 10661.0], side: Side::Good }, // 60 spawns about it
    Town { name: "Wyndhaven", world: 0, at: [4715.0, 13211.0], side: Side::Neutral }, // 54 spawns about it
    Town { name: "Gerntar Mines", world: 0, at: [13038.0, 27506.0], side: Side::Good }, // 53 spawns about it
    Town { name: "Mt. Hatespike", world: 0, at: [13764.0, 11395.0], side: Side::Evil }, // 50 spawns about it
    Town { name: "Murnf", world: 0, at: [8929.0, 10516.0], side: Side::Good }, // 50 spawns about it
    Town { name: "Dark Solace", world: 0, at: [12665.0, 21109.0], side: Side::Evil }, // 47 spawns about it
    Town { name: "Blackwater", world: 0, at: [15147.0, 22660.0], side: Side::Good }, // 46 spawns about it
    Town { name: "Castle Lightwolf", world: 0, at: [9281.0, 6504.0], side: Side::Good }, // 43 spawns about it
    Town { name: "Fort Seriak", world: 0, at: [21776.0, 8390.0], side: Side::Evil }, // 40 spawns about it
    Town { name: "Kerplunk Outpost", world: 0, at: [14815.0, 29246.0], side: Side::Evil }, // 36 spawns about it
];

/// The coach routes, each pair of towns once.
pub const ROUTES: [Route; 47] = [
    Route { from: "Arcadin", to: "Stone of Morathals (Good)", world: 2, a: [8751.3, 4137.1], b: [7235.8, 10661.0] },
    Route { from: "Blackwater", to: "Forkwatch", world: 0, a: [15151.6, 22665.0], b: [12527.8, 18760.7] },
    Route { from: "Blackwater", to: "Gerntar Mines", world: 0, a: [15151.6, 22665.0], b: [13027.1, 27501.6] },
    Route { from: "Blackwater", to: "Oasis", world: 0, a: [15151.6, 22665.0], b: [21118.3, 25173.3] },
    Route { from: "Bobble-by-Water", to: "Freeport", world: 0, a: [24640.8, 11864.5], b: [25273.7, 15719.1] },
    Route { from: "Bobble-by-Water", to: "Klik'Anon", world: 0, a: [24640.8, 11864.5], b: [23463.6, 6489.4] },
    Route { from: "Bobble-by-Water", to: "Rivervale", world: 0, a: [24640.8, 11864.5], b: [18334.8, 11357.8] },
    Route { from: "Castle Lightwolf", to: "Halas", world: 0, a: [9281.4, 6494.6], b: [12971.1, 4683.8] },
    Route { from: "Castle Lightwolf", to: "Murnf", world: 0, a: [9281.4, 6494.6], b: [8936.8, 10506.5] },
    Route { from: "Dark Solace", to: "Highpass Hold", world: 0, a: [12539.3, 21182.0], b: [16780.8, 15356.2] },
    Route { from: "Dark Solace", to: "Honjour", world: 0, a: [12539.3, 21182.0], b: [9805.1, 14990.3] },
    Route { from: "Dark Solace", to: "Kerplunk Outpost", world: 0, a: [12539.3, 21182.0], b: [14815.0, 29246.0] },
    Route { from: "Dark Solace", to: "Oasis", world: 0, a: [12539.3, 21182.0], b: [21118.3, 25173.3] },
    Route { from: "Darvar Manor", to: "Forkwatch", world: 0, a: [13603.2, 14762.7], b: [12527.8, 18760.7] },
    Route { from: "Darvar Manor", to: "Highpass Hold", world: 0, a: [13603.2, 14762.7], b: [16780.8, 15356.2] },
    Route { from: "Darvar Manor", to: "Moradhim", world: 0, a: [13603.2, 14762.7], b: [15351.6, 9267.1] },
    Route { from: "Darvar Manor", to: "Surefall Glade", world: 0, a: [13603.2, 14762.7], b: [9136.3, 12737.3] },
    Route { from: "East Plateau", to: "Stone of Morathals (Evil)", world: 2, a: [8085.3, 2143.6], b: [6847.5, 10980.8] },
    Route { from: "Fayspire", to: "Klik'Anon", world: 0, a: [19482.3, 7043.9], b: [23463.6, 6489.4] },
    Route { from: "Fayspire", to: "Moradhim", world: 0, a: [19482.3, 7043.9], b: [15351.6, 9267.1] },
    Route { from: "Fayspire", to: "Rivervale", world: 0, a: [19482.3, 7043.9], b: [18334.8, 11357.8] },
    Route { from: "Fayspire", to: "Tethelin", world: 0, a: [19482.3, 7043.9], b: [18425.7, 7494.7] },
    Route { from: "Forkwatch", to: "Highbourne", world: 0, a: [12527.8, 18760.7], b: [4858.6, 21458.7] },
    Route { from: "Forkwatch", to: "Qeynos", world: 0, a: [12527.8, 18760.7], b: [4813.4, 17143.5] },
    Route { from: "Fort Seriak", to: "Highpass Hold", world: 0, a: [21780.6, 8388.4], b: [16780.8, 15356.2] },
    Route { from: "Fort Seriak", to: "Mt. Hatespike", world: 0, a: [21780.6, 8388.4], b: [13758.4, 11398.1] },
    Route { from: "Fort Seriak", to: "Neriak", world: 0, a: [21780.6, 8388.4], b: [24925.4, 9429.4] },
    Route { from: "Freeport", to: "Highpass Hold", world: 0, a: [25273.7, 15719.1], b: [16780.8, 15356.2] },
    Route { from: "Freeport", to: "Muniel's Tea Garden", world: 0, a: [25273.7, 15719.1], b: [23369.9, 19636.2] },
    Route { from: "Freeport", to: "Neriak", world: 0, a: [25273.7, 15719.1], b: [24925.4, 9429.4] },
    Route { from: "Grobb", to: "Hazinak (Evil)", world: 0, a: [25223.0, 31175.3], b: [25128.9, 27156.3] },
    Route { from: "Grobb", to: "Kerplunk Outpost", world: 0, a: [25223.0, 31175.3], b: [14815.0, 29246.0] },
    Route { from: "Halas", to: "Moradhim", world: 0, a: [12971.1, 4683.8], b: [15351.6, 9267.1] },
    Route { from: "Hazinak (Evil)", to: "Oasis", world: 0, a: [25128.9, 27156.3], b: [21118.3, 25173.3] },
    Route { from: "Hazinak (Good)", to: "Oasis", world: 0, a: [24875.4, 27606.8], b: [21118.3, 25173.3] },
    Route { from: "Highbourne", to: "Qeynos", world: 0, a: [4858.6, 21458.7], b: [4813.4, 17143.5] },
    Route { from: "Highpass Hold", to: "Rivervale", world: 0, a: [16780.8, 15356.2], b: [18334.8, 11357.8] },
    Route { from: "Honjour", to: "Mt. Hatespike", world: 0, a: [9805.1, 14990.3], b: [13758.4, 11398.1] },
    Route { from: "Honjour", to: "Wyndhaven", world: 0, a: [9805.1, 14990.3], b: [4710.5, 13202.0] },
    Route { from: "Klik'Anon", to: "Neriak", world: 0, a: [23463.6, 6489.4], b: [24925.4, 9429.4] },
    Route { from: "Moradhim", to: "Rivervale", world: 0, a: [15351.6, 9267.1], b: [18334.8, 11357.8] },
    Route { from: "Muniel's Tea Garden", to: "Oasis", world: 0, a: [23369.9, 19636.2], b: [21118.3, 25173.3] },
    Route { from: "Murnf", to: "Wyndhaven", world: 0, a: [8936.8, 10506.5], b: [4710.5, 13202.0] },
    Route { from: "Qeynos", to: "Surefall Glade", world: 0, a: [4813.4, 17143.5], b: [9136.3, 12737.3] },
    Route { from: "Qeynos", to: "Wyndhaven", world: 0, a: [4813.4, 17143.5], b: [4710.5, 13202.0] },
    Route { from: "Surefall Glade", to: "Wyndhaven", world: 0, a: [9136.3, 12737.3], b: [4710.5, 13202.0] },
    Route { from: "Wyndhaven", to: "Zentar's Keep", world: 0, a: [4710.5, 13202.0], b: [5500.7, 6453.6] },
];
