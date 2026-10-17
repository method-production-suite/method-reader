//! Field Station: the invented feature the README and site pictures show.
//! Was tools/field-station-content.py.
//!
//! Data only, in its own file the way torture_content.rs is, so the person
//! writing screenplay never has to read layout code. The `demo` kind lays it
//! out; tests/fixture.rs pins the result; tests/field-station.test.ts checks
//! the site's scene survives into the book.
//!
//! Every name here is invented. The project rule (CLAUDE.md) is that no real
//! title, author or character name ever reaches a picture, and this script
//! exists to be pictured.
//!
//! PAGES_BEFORE then SITE_PAGES is the list of pages, each a list of
//! (kind, text) rows. A page is laid out exactly as written and the
//! generator refuses one that overflows, so the printed page numbers stay
//! where the site's markers say they are: pages 14 to 18 are
//! site/index.html's window.SCENE, word for word.
//!
//! Kinds: scene, action, character, paren, dialogue, trans.

/// One page: its (kind, text) rows.
pub type Page = &'static [(&'static str, &'static str)];

pub const TITLE: &[(f64, &str)] = &[
    (4.0, "FIELD STATION"),
    (4.5, "Written by"),
    (5.0, "the Method Reader project"),
];

/// Every page, in order.
pub fn pages() -> Vec<Page> {
    PAGES_BEFORE.iter().chain(SITE_PAGES).copied().collect()
}

// Pages 1-13: written to lead into the site's scene. These rows are the
// source of truth for the pages and the cast; the plan no longer outlines
// them.
pub const PAGES_BEFORE: &[Page] = &[
    // 1.
    &[
        ("scene", "EXT. RIDGE ROAD - DAY"),
        (
            "action",
            "Wind comes down off the ridge in long flat gusts, pushing the grass one way and then, for no reason, the other.",
        ),
        (
            "action",
            "A road climbs the ridge in switchbacks. Nobody has driven it since the washout, and the grass is taking it back.",
        ),
        (
            "action",
            "MARA, 40s, walks it with a pack on and four days in her legs. She has the face of someone who fixes things and is never thanked.",
        ),
        (
            "action",
            "DELACROIX, 50s, walks behind her with a paper map folded to the right square. He does not look at it. He looks at the road, and counts.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Thirty-one."),
        (
            "action",
            "They take the bend. Gravel lets go under Mara's boot and goes quietly over the edge.",
        ),
        ("character", "MARA"),
        ("dialogue", "You don't have to do that out loud."),
        ("character", "DELACROIX"),
        (
            "dialogue",
            "The map says forty. I'd like to know when it's lying.",
        ),
        (
            "action",
            "Cloud sits on the ridge above them like a lid somebody forgot to close.",
        ),
        ("character", "MARA"),
        ("dialogue", "How long has he been up there?"),
        ("character", "DELACROIX"),
        ("dialogue", "Three weeks. On his own."),
        ("character", "MARA"),
        ("dialogue", "They said two."),
        ("character", "DELACROIX"),
        ("dialogue", "They say a lot of things."),
        (
            "action",
            "The road ends at the washout, a bite taken out of the mountain. Mara stops where the road stops, then goes round it on the rock.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Thirty-eight."),
    ],
    // 2.
    &[
        ("scene", "EXT. FIELD STATION - DAY"),
        (
            "action",
            "MARA and DELACROIX come up the last of the slope, packs on, bent into the wind. Below them the road they walked is a thin grey line, going all the way down.",
        ),
        (
            "action",
            "The station: a prefab hut bolted to the rock, a radio mast guyed four ways, a fuel drum, a generator crouched under a tarp. Everything that can rattle, rattles.",
        ),
        ("action", "The hut door opens before they reach it."),
        (
            "action",
            "TEO, 20s, comes out in two jumpers and no coat. Thin. He has the look of someone who has rehearsed what to say to people and forgotten all of it.",
        ),
        ("character", "TEO"),
        ("dialogue", "You came."),
        ("character", "DELACROIX"),
        ("dialogue", "We said we would."),
        ("character", "TEO"),
        (
            "dialogue",
            "People say that on the radio. It's different, seeing it.",
        ),
        (
            "action",
            "He shakes Delacroix's hand, then Mara's, then Delacroix's again.",
        ),
        ("character", "MARA"),
        ("dialogue", "You're Teo."),
        ("character", "TEO"),
        (
            "dialogue",
            "I'm Teo. Sorry. Come in, it's warmer. It isn't, but it's out of the wind.",
        ),
        (
            "action",
            "The wind leans on all three of them. The mast answers it with a low note, like someone humming through their teeth.",
        ),
        ("character", "TEO (CONT'D)"),
        ("dialogue", "That's the mast. You get used to it."),
        ("character", "MARA"),
        ("dialogue", "Do you?"),
        ("character", "TEO"),
        ("dialogue", "No."),
    ],
    // 3.
    &[
        (
            "action",
            "Teo walks them round the station at a pace that makes it hard to see any of it.",
        ),
        ("character", "TEO"),
        (
            "dialogue",
            "Fuel. Water. Rain gauge. It's been fine the whole time.",
        ),
        ("action", "He points up at the mast without looking at it."),
        ("character", "TEO (CONT'D)"),
        (
            "dialogue",
            "It sings in a north wind. One long note. In a west wind it's two, and they don't agree.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "And an east wind?"),
        ("character", "TEO"),
        ("dialogue", "We don't get an east wind."),
        ("action", "He thinks about it."),
        ("character", "TEO (CONT'D)"),
        ("dialogue", "We got one once."),
        (
            "action",
            "Mara crouches by the generator and lifts the tarp. Oil on the housing, old and new. One bolt is missing. Another turns in her fingers with no thread left to hold it.",
        ),
        (
            "action",
            "She looks at it for a long moment. Then she lets the tarp fall.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Well?"),
        ("character", "MARA"),
        ("dialogue", "It runs."),
        (
            "action",
            "That is all she says. Delacroix has worked with her long enough to count what she leaves out.",
        ),
        ("character", "TEO"),
        ("dialogue", "I kept it running. Every day. I wrote it down."),
        ("character", "MARA"),
        ("dialogue", "I believe you."),
        (
            "action",
            "Teo nods too many times and goes inside. The wind shuts the door behind him, hard.",
        ),
    ],
    // 4.
    &[
        ("scene", "INT. FIELD STATION - DAY"),
        (
            "action",
            "One room. A cot, a table, one chair. A tarp lashed over a gap in the north wall where a panel used to be. It breathes with the weather.",
        ),
        (
            "action",
            "On the wall above the radio, a map of the valley. Four pins in it, running down the line to the lowest station.",
        ),
        ("character", "DELACROIX"),
        ("paren", "(into the handset)"),
        ("dialogue", "Base, this is Field Station. We're up."),
        ("action", "Static. Then a voice, patient, a long way off."),
        ("character", "IVERSEN (V.O.)"),
        (
            "dialogue",
            "Field Station, Iversen. Good to hear you. How's the boy?",
        ),
        (
            "action",
            "Delacroix glances at Teo, who is shelving their tins in order of size.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Tidy."),
        ("character", "IVERSEN (V.O.)"),
        (
            "dialogue",
            "Pickup is tomorrow, first light. Relief team brings the new keeper up and the three of you down.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "First light."),
        ("character", "IVERSEN (V.O.)"),
        (
            "dialogue",
            "Weather permitting. It always permits, until it doesn't.",
        ),
        (
            "action",
            "He sets the handset down. Mara is looking at the map.",
        ),
        ("character", "MARA"),
        ("dialogue", "Why four pins?"),
        ("character", "TEO"),
        (
            "dialogue",
            "One for every station on the line. We're the top one.",
        ),
        (
            "action",
            "He touches the lowest pin, then takes his finger away as if it were warm.",
        ),
    ],
    // 5.
    &[
        ("scene", "EXT. RADIO MAST - DUSK"),
        (
            "action",
            "The light goes early up here. What is left of it is the colour of weak tea.",
        ),
        (
            "action",
            "MARA climbs the mast, harness clipped to the ladder, a spanner through her belt. The wind pulls at her like it wants her for something.",
        ),
        (
            "action",
            "DELACROIX stands at the foot, the safety line paid out through his gloves.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "How many rungs?"),
        ("character", "MARA"),
        ("dialogue", "I don't count them."),
        ("character", "DELACROIX"),
        ("dialogue", "Forty-four. I counted for you."),
        (
            "action",
            "She reaches the antenna. It hangs off true, its bracket worked loose by a winter of north wind.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Can I ask you something?"),
        ("character", "MARA"),
        ("dialogue", "You're going to."),
        ("character", "DELACROIX"),
        (
            "dialogue",
            "Why did you take this job? You could fix things anywhere. Somewhere with walls.",
        ),
        (
            "action",
            "A gust takes the mast and leans it an inch. She waits it out, cheek against the cold steel, eyes shut.",
        ),
        (
            "action",
            "Below her, the line goes taut in his hands and stays taut until the gust is gone.",
        ),
    ],
    // 6.
    &[
        ("character", "MARA"),
        ("dialogue", "Nothing up here pretends to work."),
        (
            "action",
            "Delacroix thinks about that. Decides not to agree with it out loud.",
        ),
        (
            "action",
            "She sets the spanner to the bracket. Two turns. The antenna comes true, and the mast's low note changes pitch, very slightly, as if it has noticed.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Is that it?"),
        ("character", "MARA"),
        ("dialogue", "That's it."),
        (
            "action",
            "She doesn't come down. She is looking past him, past the hut, out across the valley to the far ridge.",
        ),
        (
            "action",
            "A light. One small steady point of it, low on the far slope, where the map shows nothing at all.",
        ),
        (
            "action",
            "It does not flicker. Nothing out there has any reason to be lit.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Mara? You're losing the light."),
        (
            "action",
            "The point goes out, like somebody closing a hand around it.",
        ),
        ("character", "MARA"),
        ("dialogue", "Coming down."),
        (
            "action",
            "She unclips and starts down, rung by rung. She does not mention it. She will not mention it for some time.",
        ),
    ],
    // 7.
    &[
        ("scene", "INT. FIELD STATION - EVENING"),
        (
            "action",
            "Three of them round a table built for one. Tins on a camping stove. The tarp on the north wall breathes in, out.",
        ),
        (
            "action",
            "TEO eats fast, like the food might be taken back.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Four pins. Tell me about them."),
        ("character", "TEO"),
        (
            "dialogue",
            "We call down the line at six every evening. Everybody answers. We all say goodnight.",
        ),
        ("character", "MARA"),
        ("dialogue", "Everybody?"),
        ("action", "Teo puts his fork down."),
        ("character", "TEO"),
        ("dialogue", "The fourth hasn't answered. Nine days."),
        ("action", "A pause. The stove ticks as it cools."),
        ("character", "DELACROIX"),
        ("dialogue", "Nine days, and you didn't report it."),
        ("character", "TEO"),
        (
            "dialogue",
            "I did. Every night, after I called them, I reported it to base.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "And?"),
        ("character", "TEO"),
        ("dialogue", "Nobody answered that either."),
        (
            "action",
            "Delacroix looks at Mara. Mara looks at the radio. The radio hisses and says nothing, the way it has said nothing for nine days.",
        ),
        ("character", "MARA"),
        ("dialogue", "Iversen answered today."),
        ("character", "TEO"),
        ("dialogue", "Iversen answered you."),
    ],
    // 8.
    &[
        ("scene", "EXT. RIDGE - NIGHT"),
        (
            "action",
            "Wind, and under the wind a cold that means business.",
        ),
        (
            "action",
            "DELACROIX walks the ridge with a torch, the beam no use against anything further off than his own boots.",
        ),
        (
            "action",
            "He finds the marker cairn where the map says it will be: a chest-high stack of flat stones, sighted down the valley toward the fourth station. In clear weather you could see its lamp from here.",
        ),
        (
            "action",
            "There is no lamp tonight. There is no clear weather.",
        ),
        (
            "action",
            "He sets the torch on the ground and looks at the cairn properly.",
        ),
        (
            "action",
            "It has been rebuilt. Carefully. Every stone seated, every gap chinked with smaller stones. Nobody did this in a hurry, and nobody did it for the weather.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Hello?"),
        (
            "action",
            "The wind takes the word and does not bring it back.",
        ),
        (
            "action",
            "He knows this cairn. He built the last one himself, eleven years ago, and he knows how many stones went into it. Twenty-six.",
        ),
    ],
    // 9.
    &[
        ("action", "He counts them. His lips move."),
        ("character", "DELACROIX"),
        ("dialogue", "Twenty-five."),
        (
            "action",
            "He counts again, slower, touching each one with a gloved finger, top to bottom.",
        ),
        ("character", "DELACROIX (CONT'D)"),
        ("dialogue", "Twenty-five."),
        (
            "action",
            "One short. Whoever rebuilt it built it well and built it one stone short, and a careful person does not make that mistake by accident.",
        ),
        (
            "action",
            "He sweeps the torch around his feet. Grass. Rock. No stone lying loose.",
        ),
        (
            "action",
            "Down in the valley, where the fourth station should be, there is only dark.",
        ),
        (
            "action",
            "He switches the torch off and stands in it for a moment, to see what his eyes will do.",
        ),
        (
            "action",
            "Nothing out there changes. Nothing out there has changed for nine days.",
        ),
        (
            "action",
            "He turns the torch back on and walks back toward the hut, faster than he came.",
        ),
    ],
    // 10.
    &[
        ("scene", "INT. FIELD STATION - LATER"),
        (
            "action",
            "Mara is asleep on the cot in her boots. Teo is not asleep, and not pretending to be.",
        ),
        (
            "action",
            "The radio crackles. Delacroix comes in with the cold still on him and goes straight to the handset.",
        ),
        ("character", "IVERSEN (V.O.)"),
        (
            "dialogue",
            "Field Station, Iversen. Field Station, do you read?",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Go ahead."),
        ("character", "IVERSEN (V.O.)"),
        (
            "dialogue",
            "Weather's closing over the pass. Relief is delayed.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Delayed how long?"),
        ("action", "Static, for long enough that Teo sits up."),
        ("character", "IVERSEN (V.O.)"),
        (
            "dialogue",
            "Twenty-three hours, earliest. We'll hold here until it...",
        ),
        ("action", "The hiss closes over the rest."),
        ("character", "DELACROIX"),
        ("dialogue", "Iversen. Come back."),
        (
            "action",
            "Nothing. Delacroix sets the handset down gently, as if it might still be listening.",
        ),
        (
            "action",
            "He takes a pencil from his pocket and writes 23 on the wall beside the map, small and neat. He looks at it. Underlines it.",
        ),
        ("character", "TEO"),
        ("dialogue", "Was that him? Was that really him?"),
        ("character", "DELACROIX"),
        ("dialogue", "Go to sleep, Teo."),
    ],
    // 11.
    &[
        ("scene", "EXT. FIELD STATION - NIGHT"),
        (
            "action",
            "The wind has found a new direction and does not like that one either.",
        ),
        (
            "action",
            "TEO wheels the quad bike out of the lean-to. Headlamp on. A jerrycan strapped across the back, and a second map, the survey copy, rolled under his arm.",
        ),
        ("action", "MARA comes out after him, pulling on her coat."),
        ("character", "MARA"),
        ("dialogue", "Teo."),
        ("character", "TEO"),
        (
            "dialogue",
            "It's four hours down if the track's clear. I'll see them. I'll come back and tell you.",
        ),
        ("character", "MARA"),
        ("dialogue", "Don't."),
        ("character", "TEO"),
        ("dialogue", "Nine days. Somebody has to go and look."),
        ("character", "MARA"),
        ("dialogue", "In the morning. With us."),
        ("character", "TEO"),
        (
            "dialogue",
            "In the morning there'll be a reason to wait until the afternoon.",
        ),
        (
            "action",
            "He is not wrong, and she knows it, and that is the worst part.",
        ),
        ("character", "MARA"),
        ("dialogue", "Call us when you get there."),
        ("character", "TEO"),
        ("dialogue", "I always call."),
        (
            "action",
            "He kicks the bike over. It catches first time, which feels like a verdict.",
        ),
    ],
    // 12.
    &[
        (
            "action",
            "DELACROIX is in the doorway now, the lamp behind him.",
        ),
        ("character", "DELACROIX"),
        (
            "dialogue",
            "Four hours down. Four back. The relief is twenty-three.",
        ),
        ("character", "TEO"),
        ("dialogue", "Then I'll be back before them."),
        (
            "action",
            "Teo pulls out onto the track. The headlamp swings across the rock, across the fuel drum, and settles on the way down.",
        ),
        (
            "action",
            "They watch the tail light go. Red, small, smaller. It dips below the lip of the ridge, comes up once more on the next switchback, and then does not come up again.",
        ),
        ("action", "Mara keeps looking at the place where it was."),
        ("character", "DELACROIX"),
        ("dialogue", "You could have stopped him."),
        ("character", "MARA"),
        ("dialogue", "I could have slowed him down."),
        (
            "action",
            "Behind them, under its tarp, the generator coughs.",
        ),
        (
            "action",
            "Once. A dry, small sound, like someone clearing their throat before bad news. Then it runs on, even and ordinary, as if nothing happened.",
        ),
        (
            "action",
            "Neither of them says anything. Delacroix looks at his watch and does not say the time.",
        ),
        ("trans", "CUT TO:"),
    ],
    // 13.
    &[
        ("scene", "EXT. LOWER STATION - NIGHT"),
        (
            "action",
            "A hut like theirs, lower and older, in a fold of the valley where the wind cannot quite reach and so sulks instead.",
        ),
        (
            "action",
            "A headlamp comes down the track and stops. TEO kills the engine. The silence is enormous.",
        ),
        (
            "action",
            "The door of the hut is open. Not blown open. Propped, with a stone.",
        ),
        ("character", "TEO"),
        ("dialogue", "Hello? It's Teo. From the top."),
        ("action", "Nothing. He goes in."),
        (
            "action",
            "Inside, the station is neat. The cot is made. A mug stands on the table with a skin on its cold tea.",
        ),
        (
            "action",
            "The radio is on. Its dial glows a soft green, the only light in the room.",
        ),
        (
            "action",
            "Teo leans in to read the frequency. He knows the number. It is theirs.",
        ),
        (
            "action",
            "Beside the radio a notebook lies open. Line after line of the same entry, in a hand that gets smaller down the page. Six o'clock. Six o'clock. Six o'clock.",
        ),
        (
            "action",
            "Someone has been listening. Every night. And never once said a word.",
        ),
        (
            "action",
            "Teo picks up the handset. His thumb finds the key.",
        ),
        ("action", "He holds it down."),
    ],
];

// Pages 14-18: site/index.html window.SCENE, word for word. Split at the
// site's own ['pg', ...] markers. Curly apostrophes would be fine here:
// the generator's ASCII_MAP straightens them, as it does for every
// fixture.
pub const SITE_PAGES: &[Page] = &[
    // 14.
    &[
        ("scene", "INT. FIELD STATION - NIGHT"),
        (
            "action",
            "The generator coughs once and dies. MARA works by the light of her phone, hands steady, and does not look up.",
        ),
        ("character", "MARA"),
        ("dialogue", "Give it a minute. It always comes back."),
        ("character", "DELACROIX"),
        ("dialogue", "And if it doesn't?"),
        (
            "action",
            "She finds the housing bolt by feel. Turns it a quarter turn.",
        ),
        ("character", "MARA"),
        ("dialogue", "Then we do this in the dark."),
        (
            "action",
            "The wind takes the tarp on the north wall and holds it open like a door nobody opened.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "That's not an answer."),
        ("character", "MARA"),
        ("dialogue", "It's the one I have."),
        (
            "action",
            "He crosses to the window. Outside, the ridge is a black line against a slightly less black sky.",
        ),
        ("character", "DELACROIX"),
        (
            "dialogue",
            "Nineteen hours until the relief team. I counted.",
        ),
        ("character", "MARA"),
        ("dialogue", "Then stop counting."),
    ],
    // 15.
    &[
        ("action", "The generator catches. Dies again. Catches."),
        (
            "action",
            "Light comes up amber and unsteady, and the room assembles itself around them: the cot, the radio, the map with its four pins.",
        ),
        ("character", "MARA (CONT'D)"),
        ("dialogue", "There. Told you."),
        ("character", "DELACROIX"),
        ("dialogue", "You told me a minute. That was six."),
        ("character", "MARA"),
        ("dialogue", "I rounded."),
        (
            "action",
            "She wipes her hands on her thigh and sits, finally, like the sitting costs her something.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "You should sleep."),
        ("character", "MARA"),
        ("dialogue", "You should stop saying that."),
        (
            "action",
            "A long moment. The radio hisses, says nothing, hisses.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "If the relief doesn't come."),
        ("character", "MARA"),
        ("dialogue", "They'll come."),
        ("character", "DELACROIX"),
        ("dialogue", "If."),
    ],
    // 16.
    &[
        ("character", "MARA"),
        (
            "dialogue",
            "Then we walk out the way we walked in, and it takes four days instead of one, and you complain the entire time.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "I don't complain."),
        ("character", "MARA"),
        ("dialogue", "You're complaining now."),
        (
            "action",
            "He almost laughs. It is the first honest sound either of them has made all night.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "Get some sleep. I'll take the radio."),
        (
            "action",
            "She doesn't argue, which frightens him more than the dark did.",
        ),
        ("trans", "CUT TO:"),
        ("scene", "INT. FIELD STATION - DAWN"),
        (
            "action",
            "Grey light. The tarp is down. The map is gone from the wall.",
        ),
    ],
    // 17.
    &[
        (
            "action",
            "DELACROIX sits exactly where he sat, the handset still in his fist, asleep with his eyes open the way soldiers learn.",
        ),
        (
            "action",
            "The radio speaks. One word, clipped, in a voice neither of them has heard.",
        ),
        ("action", "He is awake before he knows he is awake."),
        ("character", "DELACROIX"),
        ("dialogue", "Say again."),
        (
            "action",
            "Nothing. The hiss closes over the word like water.",
        ),
        ("character", "DELACROIX (CONT'D)"),
        ("dialogue", "Say again."),
        (
            "action",
            "He turns the dial one notch, then back, then one notch the other way, hunting the frequency the way you hunt a name you have almost remembered.",
        ),
        ("character", "MARA (O.S.)"),
        ("dialogue", "What was it?"),
    ],
    // 18.
    &[
        (
            "action",
            "She is in the doorway. She has not slept and does not pretend otherwise.",
        ),
        ("character", "DELACROIX"),
        ("dialogue", "A voice. One word. I couldn't hold it."),
        ("character", "MARA"),
        ("dialogue", "Which word?"),
        ("action", "He looks at the handset, then at her."),
        ("character", "DELACROIX"),
        ("dialogue", "Ours."),
        (
            "action",
            "Outside, very far off, something that is not the wind moves along the ridge.",
        ),
        ("trans", "FADE OUT."),
    ],
];
