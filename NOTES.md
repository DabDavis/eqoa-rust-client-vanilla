# lane-setup notes

Done (crates/eqoa/src/pregame/setup/):
- mod.rs: VISetup::Process's state machine (legal -> licence -> [movie left out] -> title), the focus,
  the title's windows (Options = client's options::WndOptions, the same code as SETUP's; message box =
  textlist::Message, CLIENT's port of the same VIWndMessage), RunPatcher's closing fade, the title music.
- legal.rs, eula.rs, splash.rs, splashfx.rs, fader.rs, netsetup.rs (the client's Network Setup page).
- ui.rs: Layout::solid and Layout::textured (the fader's blended black, the island's float quad).

For the coordinator (host.rs/viewer.rs are not this lane's):
- Setup::take_sound() -> Vec<OptionsUp>: Volume/Output, to apply as play.rs does
  (s.dev.set_sound_class_volume / set_output_mode) each frame while the stage is Setup.
- Setup::capturing()/capture_key(name)/capture_button(bit): the Controls page in the title's Options.

Judgment calls: see the report.
