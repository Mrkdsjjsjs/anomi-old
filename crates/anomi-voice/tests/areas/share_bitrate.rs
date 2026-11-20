//! The stream's bitrate limit with Settings' "Unlocked bitrate", and
//! without it: the steps past 9 Mbit/s and "Custom value…" in the gear's
//! "Advanced", the stream's menu and the menu of one's own stream's tile; a
//! limit kept past 9 shown as 9 while locked; the summaries saying the
//! limit; and the limit typed by hand, its field, "Apply", "Cancel" and
//! why it refuses a value.

use iced::Size;
use iced::keyboard::key::Named;
use iced_test::{Simulator, selector};

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::share as samples;
use anomi_vm::menu::Row;
use anomi_vm::voice::share::entry::Refusal;
use anomi_vm::voice::share::{Entry, Intent, Share};
use anomi_voice::share::{self, built, gear, own_menu};

use super::support::{asked, front, settings};

/// Room for the dialog, and for the stream's menu with its long submenu.
const ROOM: Size = Size::new(1000.0, 1100.0);

fn simulate(vm: &Share) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), ROOM, share::view(vm, front()))
}

/// The limit's rows under "BITRATE LIMIT", to the next group's title: each
/// row's text and whether it's chosen.
fn limits<I>(rows: &[Row<I>]) -> Vec<(&str, bool)> {
    (rows.iter())
        .skip_while(|row| row.text != "BITRATE LIMIT")
        .skip(1)
        .take_while(|row| !row.heading)
        .map(|row| (row.text.as_str(), row.checked))
        .collect()
}

/// The gear's "Advanced" rows.
fn advanced(vm: &Share) -> Vec<Row> {
    gear(vm)
        .into_iter()
        .find(|row| row.text == "Advanced Options")
        .expect("Advanced")
        .submenu
}

#[test]
fn locked_the_steps_stop_at_9_and_a_limit_kept_past_it_shows_as_9() {
    let vm = built(Share {
        max_kbps: Some(20_000),
        ..samples::tuned()
    });
    let rows = advanced(&vm);
    let shown = limits(&rows);
    let texts: Vec<&str> = shown.iter().map(|(text, _)| *text).collect();
    assert_eq!(
        texts,
        ["Auto", "2 Mbit/s", "3.5 Mbit/s", "5 Mbit/s", "6 Mbit/s", "8 Mbit/s", "9 Mbit/s"]
    );
    assert!(shown.contains(&("9 Mbit/s", true)), "{shown:?}");
    assert_eq!(vm.max_kbps, Some(20_000), "kept as it was");
    assert_eq!(vm.rows.quality, "720p · 60fps · System sound", "as before");
}

#[test]
fn unlocked_the_steps_go_to_50_and_the_summary_says_the_limit() {
    let vm = built(samples::unlocked());
    let rows = advanced(&vm);
    let shown = limits(&rows);
    let texts: Vec<&str> = shown.iter().map(|(text, _)| *text).collect();
    assert_eq!(
        texts,
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
    let chosen: Vec<&str> = (shown.iter())
        .filter(|(_, checked)| *checked)
        .map(|(text, _)| *text)
        .collect();
    assert_eq!(chosen, ["20 Mbit/s"]);
    let custom = rows.iter().find(|row| row.text == "Custom value…").unwrap();
    assert_eq!(custom.subtitle, "From 1 to 50 Mbit/s");
    assert_eq!(
        custom.intent,
        Some(anomi_vm::Intent::from(Intent::CustomBitrate))
    );
    assert_eq!(vm.rows.quality, "720p · 60fps · 20 Mbit/s · System sound");
    // A limit typed by hand: "Custom value…" is chosen, saying it.
    let typed = built(Share {
        max_kbps: Some(20_500),
        ..samples::unlocked()
    });
    let rows = advanced(&typed);
    let custom = rows.iter().find(|row| row.text == "Custom value…").unwrap();
    assert!(custom.radio && custom.checked);
    assert_eq!(custom.subtitle, "20.5 Mbit/s");
    assert_eq!(limits(&rows).iter().filter(|(_, checked)| *checked).count(), 1);
    // One's own stream's tile has them under "More Settings".
    let own = own_menu(&typed, true);
    let more = own.iter().find(|row| row.text == "More Settings").unwrap();
    assert!(limits(&more.submenu).contains(&("Custom value…", true)));
}

#[test]
fn the_streams_menu_says_the_limit_and_offers_the_entry() {
    let vm = built(samples::live_unlocked());
    let live = vm.live.as_ref().unwrap();
    assert_eq!(live.quality.subtitle, "720p · 60 fps · 20.5 Mbit/s");
    let locked = built(samples::live_quality());
    assert_eq!(
        locked.live.as_ref().unwrap().quality.subtitle,
        "720p · 60 fps",
        "as before without the unlock"
    );
    let mut ui = simulate(&vm);
    ui.click("Custom value…").expect("among the limit's steps");
    assert_eq!(asked(ui), [Intent::CustomBitrate]);
    let mut ui = simulate(&vm);
    ui.click("30 Mbit/s").expect("a step past 9");
    assert_eq!(asked(ui), [Intent::Bitrate(Some(30_000))]);
}

#[test]
fn the_entry_shows_in_the_dialogs_place_and_says_why_it_refuses() {
    let vm = built(samples::typing());
    let mut ui = simulate(&vm);
    for shown in ["Bitrate limit", "Mbit/s", "The limit goes from 1 to 50 Mbit/s.", "Apply"] {
        assert!(ui.find(shown).is_ok(), "{shown}");
    }
    assert!(ui.find("Applications").is_err(), "in the dialog's place");
    ui.click("Apply").unwrap();
    assert_eq!(asked(ui), [Intent::ApplyBitrate]);
    let mut ui = simulate(&vm);
    ui.click("Cancel").unwrap();
    assert_eq!(asked(ui), [Intent::CancelBitrate]);
    // Typed in, and Enter sets it.
    let empty = built(Share {
        entry: Some(Entry::default()),
        ..samples::unlocked()
    });
    let mut ui = simulate(&empty);
    ui.click(selector::id(share::entry::FIELD)).unwrap();
    ui.typewrite("7");
    ui.tap_key(Named::Enter);
    assert_eq!(
        asked(ui),
        [Intent::BitrateText("7".into()), Intent::ApplyBitrate]
    );
    // Each refusal says why in words of its own.
    for refused in [Refusal::Empty, Refusal::NotANumber] {
        let vm = built(Share {
            entry: Some(Entry {
                text: String::new(),
                refused: Some(refused),
            }),
            ..samples::unlocked()
        });
        let mut ui = simulate(&vm);
        assert!(ui.find("The limit goes from 1 to 50 Mbit/s.").is_err());
        assert!(
            ui.find("Type how many Mbit/s, such as 20.").is_ok()
                || ui.find("Type a number, such as 20 or 12.5.").is_ok()
        );
    }
}
