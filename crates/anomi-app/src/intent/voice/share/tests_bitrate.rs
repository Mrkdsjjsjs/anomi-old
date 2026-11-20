//! The bitrate limit with Settings' "Unlocked bitrate", on the real core
//! in the preview: a limit kept past 9 Mbit/s counts as 9 without the
//! unlock and isn't lost, and the unlock brings it back; with it the steps
//! go to 50 Mbit/s, and "Custom value…" opens the limit typed by hand,
//! which Enter sets, which says why it refuses a value, which Escape
//! leaves, and which opens alone from the menu of one's own stream's tile.

use anomi_vm::menu::{Row, Spot};
use anomi_vm::shell::{self, Layer};
use anomi_vm::voice::share::Intent;
use anomi_vm::voice::share::entry::Refusal;
use anomi_vm::voice::tiles;
use yapfast_core::voice::SharePhase;

use super::tests::{ask, call, done};
use crate::app::App;

/// Whether a window menu has a row of `text`, in its submenus too.
fn has(rows: &[Row], text: &str) -> bool {
    (rows.iter()).any(|row| row.text == text || has(&row.submenu, text))
}

/// The texts of the limit's rows, under "BITRATE LIMIT" to the next
/// group's title.
fn limits(rows: &[Row<Intent>]) -> Vec<&str> {
    (rows.iter())
        .skip_while(|row| row.text != "BITRATE LIMIT")
        .skip(1)
        .take_while(|row| !row.heading)
        .map(|row| row.text.as_str())
        .collect()
}

/// Turns the unlock on or off as Settings does, but for the workers'
/// switch, which every test in this binary shares.
fn unlock(app: &mut App, on: bool) {
    app.layout
        .edit(|saved| saved.stream_bitrate_unlocked = on);
    super::unlocked(app, on);
    app.refresh();
}

#[test]
fn a_limit_past_9_counts_as_9_locked_is_kept_and_comes_back() {
    let (mut app, folder) = call("bitrate-kept", Some(SharePhase::Live));
    app.core.state.voice.quality.max_kbps = Some(20_000);
    app.vm.voice.share = anomi_voice::share::built(super::picks(&mut app));
    let share = &app.vm.voice.share;
    assert!(!share.unlocked, "off unless turned on");
    assert_eq!((share.max_kbps, share.limit()), (Some(20_000), Some(9_000)));
    assert_eq!(share.rows.quality, "1080p · 30fps · System sound");
    ask(&mut app, Intent::Fps(60));
    assert_eq!(
        app.core.state.voice.quality.max_kbps,
        Some(20_000),
        "a change of quality keeps the limit as it was kept"
    );
    app.refresh();
    assert_eq!(app.vm.voice.dock.share_quality, "1080p · 60 fps");
    unlock(&mut app, true);
    let share = &app.vm.voice.share;
    assert_eq!(share.limit(), Some(20_000), "back");
    assert_eq!(share.rows.quality, "1080p · 60fps · 20 Mbit/s · System sound");
    assert_eq!(
        app.vm.voice.dock.share_quality,
        "1080p · 60 fps · 20 Mbit/s"
    );
    unlock(&mut app, false);
    assert_eq!(app.vm.voice.share.limit(), Some(9_000));
    assert_eq!(app.vm.voice.dock.share_quality, "1080p · 60 fps");
    assert_eq!(app.core.state.voice.quality.max_kbps, Some(20_000));
    done(app, folder);
}

#[test]
fn unlocked_the_steps_go_to_50_and_a_limit_is_typed_by_hand() {
    let (mut app, folder) = call("bitrate-typed", Some(SharePhase::Live));
    app.layout
        .edit(|saved| saved.stream_bitrate_unlocked = true);
    drop(super::open(&mut app, Spot::default()));
    assert!(app.vm.shell.is_open(Layer::Share), "the stream's menu");
    let live = app.vm.voice.share.live.as_ref().expect("live");
    assert_eq!(
        limits(&live.quality.submenu),
        [
            "Auto",
            "2 Mbit/s",
            "3.5 Mbit/s",
            "5 Mbit/s",
            "6 Mbit/s",
            "8 Mbit/s",
            "9 Mbit/s",
            "12 Mbit/s",
            "15 Mbit/s",
            "20 Mbit/s",
            "30 Mbit/s",
            "50 Mbit/s",
            "Custom value…"
        ]
    );
    ask(&mut app, Intent::CustomBitrate);
    let share = &app.vm.voice.share;
    assert_eq!(share.entry.as_ref().map(|entry| entry.text.as_str()), Some(""));
    assert!(!app.shown.voice.share.entry_alone, "in the menu's place");
    // A value past 50 is refused, and says why until the next edit.
    ask(&mut app, Intent::BitrateText("75".into()));
    ask(&mut app, Intent::ApplyBitrate);
    let refused = app.vm.voice.share.entry.as_ref().and_then(|entry| entry.refused);
    assert_eq!(refused, Some(Refusal::OutOfRange));
    assert_eq!(app.core.state.voice.quality.max_kbps, None, "nothing set");
    ask(&mut app, Intent::BitrateText("20,5".into()));
    assert_eq!(app.vm.voice.share.entry.as_ref().unwrap().refused, None);
    ask(&mut app, Intent::ApplyBitrate);
    assert!(app.vm.voice.share.entry.is_none());
    assert_eq!(
        app.core.state.voice.quality.max_kbps,
        Some(20_500),
        "the stream takes it at once"
    );
    assert!(app.vm.shell.is_open(Layer::Share), "back to the menu");
    let live = app.vm.voice.share.live.as_ref().unwrap();
    assert_eq!(live.quality.subtitle, "1080p · 30 fps · 20.5 Mbit/s");
    let custom = (live.quality.submenu.iter())
        .find(|row| row.text == "Custom value…")
        .unwrap();
    assert!(custom.checked && custom.subtitle == "20.5 Mbit/s");
    // Escape, as the layer's close, leaves a new entry and the limit.
    ask(&mut app, Intent::CustomBitrate);
    assert_eq!(app.vm.voice.share.entry.as_ref().unwrap().text, "20.5");
    drop(crate::intent::shell::handle(
        &mut app,
        shell::Intent::Close(Layer::Share),
    ));
    assert!(app.vm.voice.share.entry.is_none());
    assert!(app.vm.shell.is_open(Layer::Share), "the menu stays");
    assert_eq!(app.core.state.voice.quality.max_kbps, Some(20_500));
    done(app, folder);
}

#[test]
fn from_the_tiles_menu_the_entry_opens_alone_and_goes_with_its_layer() {
    let (mut app, folder) = call("bitrate-alone", Some(SharePhase::Live));
    let own = |app: &mut App| {
        let intent = anomi_vm::voice::Intent::Tiles(tiles::Intent::OwnMenu(Spot::default()));
        drop(crate::intent::voice::handle(app, intent));
    };
    own(&mut app);
    assert!(!has(&app.vm.menu.rows, "Custom value…"));
    assert!(!has(&app.vm.menu.rows, "12 Mbit/s"));
    ask(&mut app, Intent::CustomBitrate);
    assert!(app.vm.voice.share.entry.is_none(), "nothing without the unlock");
    app.layout
        .edit(|saved| saved.stream_bitrate_unlocked = true);
    own(&mut app);
    let more = (app.vm.menu.rows.iter())
        .find(|row| row.text == "More Settings")
        .unwrap();
    assert!(has(&more.submenu, "Custom value…"));
    assert!(has(&more.submenu, "50 Mbit/s"));
    assert!(!app.vm.shell.is_open(Layer::Share));
    ask(&mut app, Intent::CustomBitrate);
    assert!(!app.vm.menu.open, "the tile's menu closes");
    assert!(app.vm.shell.is_open(Layer::Share), "the layer opens for it");
    assert!(app.shown.voice.share.entry_alone);
    ask(&mut app, Intent::BitrateText("12".into()));
    ask(&mut app, Intent::ApplyBitrate);
    assert_eq!(app.core.state.voice.quality.max_kbps, Some(12_000));
    assert!(!app.vm.shell.is_open(Layer::Share), "and closes with it");
    done(app, folder);
}

/// The menus' numbers for the limit are the core's.
#[test]
fn the_menus_numbers_are_the_cores() {
    use anomi_vm::voice::share::{DOCUMENTED_KBPS, UNLOCKED_KBPS};
    assert_eq!(DOCUMENTED_KBPS, yapfast_core::voice::DOCUMENTED_KBPS);
    assert_eq!(UNLOCKED_KBPS, yapfast_core::voice::UNLOCKED_KBPS);
}
