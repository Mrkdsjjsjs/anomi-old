//! The dialog's picks: the stream modes as presets, the tabs and what the
//! button at the bottom right sends.

use super::*;

/// Two screens and a window listed, and two cameras.
fn listed() -> Share {
    Share {
        screens: vec![Screen::default(), Screen::default()],
        windows: vec!["Terminal".into()],
        cameras: vec!["Webcam".into(), "Capture card".into()],
        ..Share::default()
    }
}

#[test]
fn a_preset_is_its_resolution_and_frame_rate_and_the_rest_is_custom() {
    assert_eq!(Mode::of(Resolution::P1440, 60), Mode::Games);
    assert_eq!(Mode::of(Resolution::Source, 15), Mode::Screen);
    assert_eq!(Mode::of(Resolution::P720, 60), Mode::Custom);
    assert_eq!(Mode::of(Resolution::P1080, 60), Mode::Custom);
    assert_eq!(Mode::of(Resolution::P1440, 30), Mode::Custom);
    assert_eq!(
        Mode::Games.preset(),
        Some((Resolution::P1440, 60)),
        "Discord's"
    );
    assert_eq!(Mode::Screen.preset(), Some((Resolution::Source, 15)));
    assert_eq!(Mode::Custom.preset(), None);
    assert_eq!(Share::default().mode(), Mode::Custom, "1080p at 30");
}

#[test]
fn where_the_system_picks_choose_asks_it_for_either_kind() {
    let share = Share {
        dialog: true,
        source: None,
        ..Share::default()
    };
    assert_eq!(share.go(), Some(Intent::Ask(Offer::Either)));
    let switching = Share {
        switching: true,
        ..share
    };
    assert_eq!(switching.go(), Some(Intent::Ask(Offer::Either)));
}

#[test]
fn the_button_starts_what_the_tab_shown_has_picked() {
    let mut share = listed();
    assert!(matches!(share.go(), Some(Intent::GoLive { source: 0, .. })));
    share.show(Tab::Devices);
    assert_eq!(share.source, Some(3), "the first camera");
    assert!(matches!(share.go(), Some(Intent::GoLive { source: 3, .. })));
    share.source = Some(4);
    share.show(Tab::Apps);
    assert_eq!(share.source, Some(0), "a camera isn't the apps' pick");
    share.source = Some(2);
    share.show(Tab::Devices);
    share.show(Tab::Apps);
    assert_eq!(share.source, Some(0));
}

#[test]
fn without_a_camera_the_devices_tab_waits() {
    let mut share = Share {
        cameras: Vec::new(),
        ..listed()
    };
    share.show(Tab::Devices);
    assert_eq!((share.source, share.go()), (None, None));
    assert!(!share.can_go());
}

#[test]
fn switching_waits_for_a_pick_in_either_tab() {
    let mut share = Share {
        switching: true,
        source: None,
        ..listed()
    };
    assert_eq!(share.go(), None);
    share.show(Tab::Devices);
    assert_eq!(share.source, None);
    share.source = Some(4);
    assert_eq!(share.go(), Some(Intent::Switch(4)));
}

#[test]
fn a_limit_kept_past_9_counts_as_9_until_unlocked() {
    let locked = Share {
        max_kbps: Some(20_000),
        ..Share::default()
    };
    assert_eq!(locked.limit(), Some(DOCUMENTED_KBPS));
    assert_eq!(locked.steps(), BITRATES, "Discord's steps alone");
    let unlocked = Share {
        unlocked: true,
        ..locked
    };
    assert_eq!(unlocked.limit(), Some(20_000));
    assert_eq!(
        unlocked.steps().len(),
        BITRATES.len() + UNLOCKED_BITRATES.len()
    );
    assert_eq!(unlocked.steps().last(), Some(&UNLOCKED_KBPS));
    let past = Share {
        max_kbps: Some(80_000),
        ..unlocked
    };
    assert_eq!(past.limit(), Some(UNLOCKED_KBPS));
    let auto = Share {
        unlocked: true,
        ..Share::default()
    };
    assert_eq!(auto.limit(), None);
}
