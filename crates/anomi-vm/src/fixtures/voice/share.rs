//! Samples of the Go Live dialog and the stream's menus for tests and
//! shots, after the fake Discord's data: task V10's, and the owner's
//! screenshots of Discord's (`discord-golive-*.png`, `discord-own-stream-*`).
//! The dialog's states, and the Lounge's window with it open over the call.

use iced::{Point, Rectangle, Size};

use crate::Vm;
use crate::fixtures::pictures;
use crate::menu::Spot;
use crate::shell::Layer;
use crate::voice::controls::MENU_SHIFT;
use crate::voice::share::entry::Refusal;
use crate::voice::share::{Entry, Live, Part, Resolution, Screen, Share, Tab};

use super::super::shell;
use super::controls;

/// Where the system's dialog picks (Wayland's portal), as Discord's
/// `discord-golive-1-apps.png` shows it: the two kinds to share, at 720p
/// and 60 frames ("Custom"), with the system's sound.
pub fn dialog() -> Share {
    Share {
        dialog: true,
        source: None,
        resolution: Resolution::P720,
        fps: 60,
        encoders: vec!["Auto".into(), "Radeon 610M".into(), "Processor".into()],
        ..Share::default()
    }
}

/// Where Anomi lists what to share (X11, Windows): two screens with their
/// pictures, a few windows, and a camera on the other tab.
pub fn listed() -> Share {
    Share {
        screens: vec![
            Screen {
                title: "Screen 1".into(),
                picture: Some(pictures::sunset(160, 90)),
            },
            Screen {
                title: "Screen 2".into(),
                picture: None,
            },
        ],
        windows: vec!["Terminal".into(), "Browser".into(), "Notes".into()],
        cameras: vec!["Webcam".into()],
        ..Share::default()
    }
}

/// One screen alone, which sits in the middle.
pub fn one_screen() -> Share {
    Share {
        screens: vec![Screen {
            title: "Screen 1".into(),
            picture: Some(pictures::sunset(160, 90)),
        }],
        ..Share::default()
    }
}

/// "Devices": a webcam and OBS's virtual camera, the webcam picked.
pub fn devices() -> Share {
    let mut share = Share {
        cameras: vec!["Webcam".into(), "OBS Virtual Camera".into()],
        ..dialog()
    };
    share.show(Tab::Devices);
    share
}

/// "Devices" with no camera, as `discord-golive-4-devices-tab.png`.
pub fn no_devices() -> Share {
    let mut share = dialog();
    share.show(Tab::Devices);
    share
}

/// The bitrate limit at 6 Mbit/s and the higher quality on, for the gear's
/// "Advanced".
pub fn tuned() -> Share {
    Share {
        max_kbps: Some(6_000),
        high_quality: true,
        ..dialog()
    }
}

/// With Settings' "Unlocked bitrate", 20 Mbit/s chosen: the gear's
/// "Advanced" has the steps past 9 Mbit/s and "Custom value…".
pub fn unlocked() -> Share {
    Share {
        max_kbps: Some(20_000),
        unlocked: true,
        ..tuned()
    }
}

/// The limit typed by hand over the dialog, refused: 75 is past 50.
pub fn typing() -> Share {
    Share {
        entry: Some(Entry {
            text: "75".into(),
            refused: Some(Refusal::OutOfRange),
        }),
        ..unlocked()
    }
}

/// While live, a limit typed by hand, 20.5 Mbit/s, with the quality's
/// submenu open.
pub fn live_unlocked() -> Share {
    Share {
        max_kbps: Some(20_500),
        unlocked: true,
        ..live_quality()
    }
}

/// While live, after "Change Windows": what to share instead, nothing
/// picked yet; the stream goes out at 720p and 60 fps, the higher quality
/// on.
pub fn switching() -> Share {
    Share {
        switching: true,
        source: None,
        resolution: Resolution::P720,
        fps: 60,
        high_quality: true,
        ..listed()
    }
}

/// While live, the stream's own menu, as the share button opens it: 720p
/// at 60 fps, the higher quality on, all applications heard.
pub fn live() -> Share {
    Share {
        live: Some(Live::new(true)),
        ..switching()
    }
}

/// The stream's menu with its quality's submenu open beside it, as
/// Discord's client shows it.
pub fn live_quality() -> Share {
    Share {
        live: Some(Live {
            open: Some(Part::Quality),
            current: Some(Part::Quality),
            ..Live::new(true)
        }),
        ..live()
    }
}

/// The stream's menu while the stream starts: "Change Windows" waits.
pub fn live_starting() -> Share {
    Share {
        live: Some(Live::new(false)),
        ..live()
    }
}

/// The Lounge's window with the stream's menu open over the controls'
/// share button, as the golden `voice/share_menu.png` has it.
pub fn window(share: Share) -> Vm {
    let mut vm = shell::open_at(
        controls::window(controls::joined()),
        Layer::Share,
        Spot::above(Rectangle::new(
            Point::new(933.0 - MENU_SHIFT, 794.0),
            Size::new(44.0, 44.0),
        )),
    );
    vm.voice.share = share;
    vm
}

/// Where the gear sits in [`window`]'s dialog, at 1400 by 860: the
/// dialog's bottom right, 24 px in (Discord's 958 by 596 card).
pub const GEAR: Rectangle = Rectangle {
    x: 1400.0 / 2.0 + 958.0 / 2.0 - 24.0 - 40.0,
    y: 860.0 / 2.0 + 596.0 / 2.0 - 24.0 - 40.0,
    width: 40.0,
    height: 40.0,
};
