//! The Go Live dialog's intents and the stream's menus on the real core in
//! the preview, which applies a voice command to the state as it's
//! dispatched: the modes as presets, the tabs, the gear's menu, where
//! streams encode, the preview hidden, and the menu of one's own stream's
//! tile.

use std::sync::Arc;

use yapfast_core::voice::{self, SharePhase, Sharing, StreamAudio};

use anomi_vm::menu::{Row, Spot};
use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{Intent, Mode, Resolution, Share, Sound, Tab};
use anomi_vm::voice::tiles::{self, Fill};

use crate::app::App;
use crate::project::voice::tests::in_call;

/// A call in the preview, this account's stream `phase` when there is one.
pub(super) fn call(name: &str, phase: Option<SharePhase>) -> (App, std::path::PathBuf) {
    let (mut app, folder) = in_call(name);
    app.core.state.preview = true;
    app.core.state.voice.sharing = phase.map(|phase| Sharing {
        phase,
        frames: Arc::new(yapfast_core::frames::Frames::create(4, 2).expect("frames")),
        viewers: Vec::new(),
    });
    (app, folder)
}

pub(super) fn ask(app: &mut App, intent: Intent) {
    drop(super::handle(app, intent));
}

pub(super) fn done(app: App, folder: std::path::PathBuf) {
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

/// The texts of the rows marked chosen, the submenus' among them.
pub(super) fn marked(rows: &[Row]) -> Vec<String> {
    let mut found = Vec::new();
    for row in rows {
        if row.checked {
            found.push(row.text.clone());
        }
        found.extend(marked(&row.submenu));
    }
    found
}

#[test]
fn a_preset_sets_its_quality_and_custom_brings_the_persons_back() {
    let (mut app, folder) = call("golive-modes", None);
    app.vm.voice.share = anomi_voice::share::built(Share {
        resolution: Resolution::P720,
        fps: 60,
        ..Share::default()
    });
    ask(&mut app, Intent::Mode(Mode::Screen));
    let share = &app.vm.voice.share;
    assert_eq!((share.resolution, share.fps), (Resolution::Source, 15));
    assert_eq!(share.rows.mode, "Screen Share");
    assert_eq!(share.rows.quality, "Source · 15fps · System sound");
    assert_eq!(
        app.core.state.voice.quality.fps, 30,
        "not before going live"
    );
    ask(&mut app, Intent::Mode(Mode::Games));
    ask(&mut app, Intent::Mode(Mode::Custom));
    let share = &app.vm.voice.share;
    assert_eq!((share.resolution, share.fps), (Resolution::P720, 60));
    assert_eq!(share.mode(), Mode::Custom);
    done(app, folder);
}

#[test]
fn while_live_a_preset_changes_the_stream_at_once() {
    let (mut app, folder) = call("golive-mode-live", Some(SharePhase::Live));
    app.vm.voice.share = anomi_voice::share::built(super::picks(&mut app));
    ask(&mut app, Intent::Mode(Mode::Games));
    let quality = app.core.state.voice.quality;
    assert_eq!(
        (quality.resolution, quality.fps),
        (voice::Resolution::P1440, 60),
        "Discord's Games: 1440p at 60"
    );
    assert_eq!(
        app.vm.voice.share.rows.quality,
        "1440p · 60fps · System sound"
    );
    done(app, folder);
}

#[test]
fn the_tabs_pick_what_they_list() {
    let (mut app, folder) = call("golive-tabs", None);
    app.vm.voice.share = anomi_voice::share::built(Share {
        windows: vec!["Terminal".into()],
        cameras: vec!["Webcam".into()],
        ..Share::default()
    });
    ask(&mut app, Intent::Tab(Tab::Devices));
    let share = &app.vm.voice.share;
    assert_eq!((share.tab, share.source), (Tab::Devices, Some(1)));
    assert!(matches!(share.go(), Some(Intent::GoLive { source: 1, .. })));
    done(app, folder);
}

#[test]
fn the_gears_menu_follows_its_choices_and_goes_with_the_dialog() {
    let (mut app, folder) = call("golive-gear", None);
    app.vm.voice.share = anomi_voice::share::built(super::picks(&mut app));
    app.vm.shell.open(Layer::Share);
    ask(&mut app, Intent::Gear(Spot::default()));
    assert!(app.vm.menu.open);
    assert_eq!(app.vm.menu.rows[0].text, "Stream Mode");
    assert!(marked(&app.vm.menu.rows).contains(&"Custom Quality".to_owned()));
    // A radio leaves the menu open: its mark moves.
    ask(&mut app, Intent::Fps(60));
    ask(&mut app, Intent::Resolution(Resolution::P1440));
    let marks = marked(&app.vm.menu.rows);
    assert!(marks.contains(&"1440p".to_owned()), "{marks:?}");
    assert!(marks.contains(&"Games".to_owned()), "{marks:?}");
    assert!(!marks.contains(&"Custom Quality".to_owned()));
    ask(&mut app, Intent::Sound(Sound::None));
    assert!(marked(&app.vm.menu.rows).contains(&"Mute Stream Audio".to_owned()));
    assert_eq!(app.core.state.voice.stream_audio, StreamAudio::Off);
    drop(crate::intent::voice::close(&mut app, Layer::Share));
    assert!(!app.vm.menu.open, "it goes with its dialog");
    done(app, folder);
}

#[test]
fn streams_encode_where_the_gear_says_as_settings_keep_it() {
    let (mut app, folder) = call("golive-encoder", None);
    app.vm.voice.share = anomi_voice::share::built(super::picks(&mut app));
    let processor = app.vm.voice.share.encoders.len() - 1;
    assert_eq!(app.vm.voice.share.encoders[processor], "Processor");
    ask(&mut app, Intent::Encoder(processor));
    assert_eq!(app.layout.stream_encoder.as_deref(), Some("software"));
    assert_eq!(app.vm.voice.share.encoder, processor);
    ask(&mut app, Intent::Encoder(0));
    assert_eq!(app.layout.stream_encoder.as_deref(), Some("auto"));
    done(app, folder);
}

#[test]
fn a_hidden_preview_shows_no_picture_and_is_kept() {
    let (mut app, folder) = call("golive-preview", Some(SharePhase::Live));
    app.vm.voice.share = anomi_voice::share::built(super::picks(&mut app));
    ask(&mut app, Intent::HidePreview(true));
    assert!(app.layout.hide_stream_preview);
    let own = (app.vm.voice.stage.tiles.iter())
        .find(|tile| tile.key == tiles::Key::OwnScreen)
        .expect("this account's screen");
    match &own.fill {
        Fill::OwnScreen { frame, status } => {
            assert!(frame.is_none());
            assert_eq!(status, "Your stream preview is hidden");
        }
        fill => panic!("{fill:?}"),
    }
    assert!(app.shown.voice.preview_hidden);
    done(app, folder);
}

#[test]
fn the_own_streams_menu_stops_shares_sound_and_follows_the_stream() {
    let (mut app, folder) = call("golive-own", Some(SharePhase::Starting));
    drop(crate::intent::voice::handle(
        &mut app,
        anomi_vm::voice::Intent::Tiles(tiles::Intent::OwnMenu(Spot::default())),
    ));
    let rows = &app.vm.menu.rows;
    assert_eq!(rows[0].text, "Stop streaming");
    assert!(rows[0].danger);
    let more = rows.iter().find(|row| row.text == "More Settings").unwrap();
    assert!(
        more.submenu[0].disabled,
        "Change Windows waits for the stream"
    );
    assert!(marked(rows).contains(&"Share Stream Audio".to_owned()));
    ask(&mut app, Intent::Sound(Sound::None));
    assert_eq!(app.core.state.voice.stream_audio, StreamAudio::Off);
    assert!(!marked(&app.vm.menu.rows).contains(&"Share Stream Audio".to_owned()));
    if let Some(sharing) = app.core.state.voice.sharing.as_mut() {
        sharing.phase = SharePhase::Live;
    }
    drop(super::follow(&mut app));
    let more = (app.vm.menu.rows.iter())
        .find(|row| row.text == "More Settings")
        .unwrap();
    assert!(!more.submenu[0].disabled, "live: what's shared can change");
    app.core.state.voice.sharing = None;
    drop(super::follow(&mut app));
    assert!(!app.vm.menu.open, "it closes as the stream ends");
    done(app, folder);
}
