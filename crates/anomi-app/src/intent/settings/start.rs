//! What the person's settings set up as Anomi starts, before the window
//! opens, as the Slint app's `Layout::read` and `Layout::apply` do
//! (`src/ui.rs:1625`, `:1728`) and its start (`:173`-`:193`): the person's
//! own themes and the one chosen, their font files, what the call's audio
//! and the clocks take of the layout, and the desktop's entry, links and
//! start at sign-in. The processor's cores are set earlier still, before
//! the program's threads (`crate::run::place_on_cores`).

use serde_json::{Value, json};

use anomi_theme::custom::{self, Custom};
use anomi_theme::fonts::custom::{FOLDER, Loaded, Stored};

use crate::app::App;
use crate::layout::Saved;

/// Sets up what the settings keep, as Anomi starts (`preview` for the local
/// preview, which leaves the desktop alone).
pub fn start(app: &mut App, preview: bool) {
    let mut themes: Vec<Custom> = (app.layout.custom_themes.iter())
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect();
    custom::tidy(&mut themes);
    app.theme = custom::chosen(app.layout.theme(), &themes);
    let kept = &mut app.shown.settings.kept;
    kept.custom = themes;
    let fonts = stored_fonts(&app.layout.ui_fonts);
    if !fonts.is_empty() {
        let folder = app.root.join(FOLDER);
        kept.fonts = Loaded::load(&folder, Loaded::default(), &fonts);
    }
    apply(&app.layout);
    if !preview {
        // Before the window maps, as the desktop looks up its icon then.
        #[cfg(target_os = "linux")]
        anomi_desktop::desktop_entry::install();
        if cfg!(feature = "discord") && anomi_desktop::protocol::SUPPORTED {
            anomi_desktop::protocol::refresh(&app.root);
        }
        anomi_desktop::autostart::refresh();
    }
}

/// The font files `layout.json` keeps, as the Slint app writes them.
fn stored_fonts(values: &[Value]) -> Vec<Stored> {
    let text = |value: &Value, key| value.get(key)?.as_str().map(str::to_owned);
    (values.iter())
        .filter_map(|value| {
            Some(Stored {
                name: text(value, "name")?,
                file: text(value, "file")?,
            })
        })
        .collect()
}

/// A font file as `layout.json` keeps it.
pub(super) fn font_value(stored: &Stored) -> Value {
    json!({ "name": stored.name, "file": stored.file })
}

/// Hands the call's audio, the clocks, the stream's encoder and the game
/// detection what the layout says, as the Slint app's `Layout::apply`.
fn apply(saved: &Saved) {
    use yapfast_core::{leveling, noise, volume};
    super::prefs::clock(saved);
    leveling::set_enabled(saved.level_voice.unwrap_or(true));
    noise::set(saved.noise_suppression);
    noise::set_strength(saved.noise_strength);
    noise::set_dereverb(saved.reduce_room_echo);
    noise::set_echo_cancellation(saved.echo_cancellation.unwrap_or(true));
    yapfast_core::set_windows_effects(saved.windows_microphone_effects);
    let sensitivity = (saved.input_sensitivity)
        .unwrap_or(noise::DEFAULT_SENSITIVITY as i32)
        .clamp(-80, 0);
    noise::set_sensitivity(sensitivity as f32);
    let gain = |value: Option<u8>| volume::from_slider(f32::from(value.unwrap_or(100)));
    volume::set_microphone(gain(saved.microphone_volume));
    volume::set_speakers(gain(saved.speaker_volume));
    for (person, gain) in &saved.volumes {
        volume::set(volume::Kind::Voice, person, *gain);
    }
    for (person, gain) in &saved.stream_volumes {
        volume::set(volume::Kind::Stream, person, *gain);
    }
    // A stream muted from its menu plays at nothing, its volume kept above.
    for person in &saved.muted_streams {
        volume::set(volume::Kind::Stream, person, 0.0);
    }
    for person in &saved.muted_people {
        volume::mute(person, true);
    }
    for person in &saved.soundboard_muted_people {
        volume::mute_soundboard(person, true);
    }
    // Whose voice the call cleans, and how, as their menus in a call chose.
    for (person, voice) in &saved.cleaned_voices {
        volume::set_cleaning(person, voice.cleaning());
    }
    // And whose stream's sound, from the menu of its tile.
    for (person, sound) in &saved.cleaned_streams {
        volume::set_stream_cleaning(person, sound.cleaning());
    }
    #[cfg(feature = "discord")]
    {
        yapfast_core::stream_worker::set_encode_on(saved.encoder().key());
        yapfast_core::stream_worker::unlock_bitrate(saved.stream_bitrate_unlocked);
    }
    // The games named by hand, and those added from Discord's list.
    super::games::source::detect(saved);
    let moving = crate::layout::carry::cosmetics(saved).moving;
    crate::pictures::set_moving_collectibles(moving);
}
