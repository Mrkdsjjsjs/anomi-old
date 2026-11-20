//! The person's settings on the preview: Settings filled as it opens and
//! emptied once it went, a switch of each kind kept as the Slint app keeps
//! it, the theme editor's preview and what it leaves behind, the games and
//! the keybinds. No test chooses a language: it would change the text of
//! the tests running beside it.

use anomi_vm::settings::frame::{self, Page};
use anomi_vm::settings::{Intent, discord, games, keybinds, messages, sounds};
use anomi_vm::settings::{appearance, theme_editor};
use anomi_vm::shell::{self, Layer};
use yapfast_core::sounds::Sound;

use crate::app::App;
use crate::tests::{Window, start};

fn send(app: &mut App, intent: Intent) {
    drop(super::handle(app, intent));
}

/// The preview with Settings open on `page`.
fn opened(name: &str, page: Page) -> Window {
    let mut window = start(name, true);
    drop(super::open(&mut window.app, page));
    window
}

/// What the layout's file holds now.
fn written(app: &mut App) -> serde_json::Value {
    app.layout.flush();
    let file = app.root.join(yapfast_core::layout_file::LAYOUT);
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

#[test]
fn settings_open_filled_and_hold_nothing_once_gone() {
    let mut window = opened("settings-open", Page::Messages);
    let app = &mut window.app;
    let settings = &app.vm.settings;
    assert!(app.vm.shell.is_open(Layer::Settings));
    assert_eq!(settings.frame.name, app.vm.account.name);
    assert!(settings.frame.build.is_some() && !settings.frame.close.is_empty());
    assert!(settings.messages.link_previews, "the layout's defaults");
    send(app, Intent::Frame(frame::Intent::Choose(Page::Appearance)));
    let themes = &app.vm.settings.appearance.themes;
    assert_eq!(themes.len(), 12, "the built-in themes");
    assert!(themes.iter().any(|theme| theme.id == "midnight"));
    assert_eq!(app.vm.settings.appearance.theme_id, "bonfire");
    assert_eq!(app.vm.settings.appearance.languages[0], "Русский");
    send(app, Intent::Frame(frame::Intent::Close));
    assert!(!app.vm.shell.is_open(Layer::Settings));
    drop(crate::intent::handle(
        app,
        shell::Intent::Gone(Layer::Settings).into(),
    ));
    let settings = &app.vm.settings;
    assert!(settings.appearance.themes.is_empty() && settings.frame.name.is_empty());
    assert_eq!(
        settings.frame.page,
        Page::Appearance,
        "the page chosen stays"
    );
}

#[test]
fn switches_are_kept_as_the_slint_app_keeps_them() {
    let mut window = opened("settings-switches", Page::Messages);
    let app = &mut window.app;
    send(app, Intent::Messages(messages::Intent::Compact(true)));
    send(
        app,
        Intent::Messages(messages::Intent::TextSize(messages::TextSize::Large)),
    );
    send(app, Intent::Messages(messages::Intent::ReplyPing(false)));
    assert!(app.vm.settings.messages.compact, "shown at once");
    assert_eq!(
        app.vm.settings.messages.text_size,
        messages::TextSize::Large
    );
    assert!(app.vm.chat.timeline.compact || app.core.state.selected.is_none());
    send(app, Intent::Sounds(sounds::Intent::EachSettled(6, 40)));
    send(app, Intent::Sounds(sounds::Intent::EachSettled(7, 100)));
    send(
        app,
        Intent::Discord(discord::Intent::ShowHiddenChannels(true)),
    );
    assert!(app.core.state.show_hidden_channels, "the core has it");
    let file = written(app);
    assert_eq!(file["compact_messages"], true);
    assert_eq!(file["text_size"], 2);
    assert_eq!(file["reply_ping"], false);
    assert_eq!(file["show_hidden_channels"], true);
    assert_eq!(file["each_sound"], serde_json::json!({ "message": 40 }));
    assert_eq!(app.layout.each_sound.get(&Sound::Mention), None);
}

#[test]
fn the_theme_editor_previews_and_leaves_nothing_unsaved() {
    let mut window = opened("settings-editor", Page::Appearance);
    let app = &mut window.app;
    send(app, Intent::Appearance(appearance::Intent::NewTheme));
    assert!(app.vm.settings.appearance.editing);
    assert_eq!(app.vm.settings.theme_editor.name, "Bonfire copy");
    let accent = (app.vm.settings.theme_editor.colours.iter())
        .position(|row| row.name == "accent")
        .unwrap();
    let colour = |intent| Intent::ThemeEditor(intent);
    send(
        app,
        colour(theme_editor::Intent::Colour(accent, "#ff0000".into())),
    );
    assert_eq!(
        app.theme.palette().accent,
        iced::Color::from_rgb8(255, 0, 0)
    );
    // Settings closing goes back to the theme chosen, and keeps nothing.
    send(app, Intent::Frame(frame::Intent::Close));
    assert_eq!(app.theme.id(), "bonfire");
    assert!(app.layout.custom_themes.is_empty() && app.layout.theme.is_none());
    // Saved, it's the person's own and chosen.
    drop(super::open(app, Page::Appearance));
    send(app, Intent::Appearance(appearance::Intent::NewTheme));
    send(
        app,
        colour(theme_editor::Intent::Colour(accent, "#00ff00".into())),
    );
    send(app, colour(theme_editor::Intent::Name("Mine".into())));
    send(app, colour(theme_editor::Intent::Save));
    assert!(!app.vm.settings.appearance.editing);
    assert_eq!(app.layout.theme.as_deref(), Some("custom:1"));
    assert_eq!(app.layout.custom_themes.len(), 1);
    assert_eq!(app.layout.custom_themes[0]["name"], "Mine");
    let cards = &app.vm.settings.appearance.themes;
    assert!(
        cards
            .last()
            .is_some_and(|card| card.custom && card.name == "Mine")
    );
    assert_eq!(
        app.theme.palette().accent,
        iced::Color::from_rgb8(0, 255, 0)
    );
}

#[test]
fn a_game_is_saved_edited_and_refused() {
    let mut window = opened("settings-games", Page::Games);
    let app = &mut window.app;
    let game = |intent| Intent::Games(intent);
    send(app, game(games::Intent::Executable("game.exe".into())));
    send(app, game(games::Intent::Save));
    assert!(app.vm.settings.games.invalid, "a game needs its text");
    send(app, game(games::Intent::Text("A Game".into())));
    send(app, game(games::Intent::Save));
    let page = &app.vm.settings.games;
    assert!(!page.invalid && page.executable_draft.is_empty() && page.editing.is_none());
    assert_eq!(page.games.len(), 1);
    send(app, game(games::Intent::Edit(0)));
    assert_eq!(app.vm.settings.games.text_draft, "A Game");
    send(app, game(games::Intent::Text("Another".into())));
    send(app, game(games::Intent::Save));
    let kept = &app.layout.game_executables;
    assert_eq!(kept, &[("game.exe".to_owned(), "Another".to_owned())]);
    send(app, game(games::Intent::Remove(0)));
    assert!(app.layout.game_executables.is_empty() && app.vm.settings.games.games.is_empty());
}

#[test]
fn keybinds_are_added_on_the_keybinds_own() {
    let mut window = opened("settings-keybinds", Page::Keybinds);
    let app = &mut window.app;
    send(app, Intent::Keybinds(keybinds::Intent::Add));
    send(app, Intent::Keybinds(keybinds::Intent::Action(0, 3)));
    assert_eq!(app.layout.keybinds.len(), 1);
    let row = &app.vm.settings.keybinds.rows[0];
    assert_eq!(row.action, keybinds::Action::ToggleMute);
    send(app, Intent::Keybinds(keybinds::Intent::Remove(0)));
    assert!(app.layout.keybinds.is_empty() && app.vm.settings.keybinds.rows.is_empty());
}

/// Optimization: where streams encode is kept with the old switch beside
/// it for the Slint app, and all cores is kept as the mode. Auto and the
/// cores chosen by hand would move the test's own threads, so they are
/// left to the layout's tests.
#[test]
fn optimization_keeps_where_streams_encode_and_the_cores_mode() {
    use anomi_vm::settings::advanced::{CpuMode, Intent as Advanced};
    let mut window = opened("settings-optimization", Page::Advanced);
    let app = &mut window.app;
    let page = &app.vm.settings.advanced;
    assert_eq!(page.encoders.first().map(String::as_str), Some("Auto"));
    assert_eq!(page.encoders.last().map(String::as_str), Some("Processor"));
    assert_eq!(page.encoders.len(), page.encoder_keys.len());
    assert_eq!((page.encoder, page.cpu_mode), (0, CpuMode::Auto.index()));
    assert!(!page.cpu_line.is_empty() && !page.encoder_line.is_empty());
    send(app, Intent::Advanced(Advanced::Encoder("software".into())));
    assert_eq!(
        app.vm.settings.advanced.encoder,
        app.vm.settings.advanced.encoders.len() - 1,
        "shown at once"
    );
    send(app, Intent::Advanced(Advanced::Encoder(String::new())));
    send(
        app,
        Intent::Advanced(Advanced::CpuMode(CpuMode::All.index())),
    );
    assert_eq!(app.vm.settings.advanced.cpu_mode, CpuMode::All.index());
    // A core ticked while the cores aren't chosen by hand changes nothing.
    send(app, Intent::Advanced(Advanced::Core(3)));
    let file = written(app);
    assert_eq!(file["stream_encoder"], "software");
    assert_eq!(file["gpu_encoding"], false, "what the Slint app reads");
    assert_eq!(file["cpu_mode"], "all");
}

/// Optimization: "Unlocked bitrate" is off until turned on, shown at once,
/// and kept in `layout.json` only while it's on. The workers' switch it
/// also turns is the process's, which tests running beside this one set
/// as their apps start, so it isn't asserted here.
#[test]
fn optimization_keeps_the_bitrates_unlock() {
    use anomi_vm::settings::advanced::Intent as Advanced;
    let mut window = opened("settings-bitrate", Page::Advanced);
    let app = &mut window.app;
    assert!(!app.vm.settings.advanced.bitrate_unlocked);
    send(app, Intent::Advanced(Advanced::BitrateUnlocked(true)));
    assert!(app.vm.settings.advanced.bitrate_unlocked, "shown at once");
    assert_eq!(written(app)["stream_bitrate_unlocked"], true);
    send(app, Intent::Advanced(Advanced::BitrateUnlocked(false)));
    assert!(!app.vm.settings.advanced.bitrate_unlocked);
    assert!(written(app).get("stream_bitrate_unlocked").is_none());
}

/// Optimization: what draws the interface is kept as its key, marked at
/// once, and the Slint app's switch stays as it came. Only the processor
/// is chosen: a graphics adapter would start a GPU under the tests.
#[test]
fn optimization_keeps_what_draws_the_interface() {
    use anomi_vm::settings::advanced::Intent as Advanced;
    let mut window = opened("settings-renderer", Page::Advanced);
    let app = &mut window.app;
    let page = &app.vm.settings.advanced;
    let keys: Vec<&str> = page.renderers.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys.get(..2), Some(&["auto", "processor"][..]));
    assert_eq!(page.renderer, 0, "Automatic until chosen");
    assert!(!page.renderer_line.is_empty(), "what draws now");
    send(
        app,
        Intent::Advanced(Advanced::Renderer("processor".into())),
    );
    assert_eq!(app.vm.settings.advanced.renderer, 1, "shown at once");
    send(app, Intent::Advanced(Advanced::Renderer(String::new())));
    let file = written(app);
    assert_eq!(file["interface_renderer"], "processor");
    assert_eq!(file["gpu_rendering"], false, "the Slint app's, as it came");
}

#[test]
fn the_sections_search_and_a_copy_change_the_frame_alone() {
    let mut window = opened("settings-frame", Page::About);
    let app = &mut window.app;
    let edits = app.layout.edits();
    send(app, Intent::Frame(frame::Intent::Search("voi".into())));
    assert_eq!(app.vm.settings.frame.query, "voi");
    send(
        app,
        Intent::About(anomi_vm::settings::about::Intent::CopyVersion),
    );
    assert!(app.vm.settings.frame.copied && app.vm.settings.about.copied);
    let copy = app.shown.settings.kept.copied;
    drop(super::due(app, super::Due::Copied(copy)));
    assert!(!app.vm.settings.frame.copied, "said for a moment");
    assert_eq!(app.layout.edits(), edits, "nothing kept");
    assert_eq!(app.vm.settings.about.original, "Original: Yapfast");
}
