//! The dialog's and the stream's menus' shots: the Lounge's window with
//! the Go Live dialog in its middle (Discord's `discord-golive-*.png`), its
//! gear's menu open over it, the stream's menu over the controls' share
//! button, and one's own stream's tile's menu (`discord-own-stream-menu.png`);
//! with the bitrate's unlock, the steps past 9 Mbit/s and the limit typed
//! by hand.

use iced::Point;

use anomi_vm::Vm;
use anomi_vm::fixtures::voice::share as fixtures;
use anomi_vm::fixtures::voice::stage;
use anomi_vm::fixtures::{GOLDEN, Shot};
use anomi_vm::menu::{Align, Menu, Spot};
use anomi_vm::voice::share::{Resolution, Share};

use super::{built, gear, own_menu};

/// The gear's "Advanced", its last row.
const ADVANCED: usize = 7;

/// The window with `share` open, its rows made.
fn window(share: Share) -> Vm {
    let mut vm = fixtures::window(share);
    vm.voice.share = built(std::mem::take(&mut vm.voice.share));
    vm
}

/// The dialog with its gear's menu open above the gear, and the submenu
/// of row `submenu` beside it.
fn with_gear(share: Share, submenu: Option<usize>) -> Vm {
    let mut vm = window(share);
    let rows = gear(&vm.voice.share);
    vm.menu = Menu {
        submenu,
        ..Menu::new(1, Spot::above(fixtures::GEAR).align(Align::End), rows)
    };
    vm
}

/// Alice live in the Lounge, the menu of her stream's tile at the pointer
/// over it.
fn own_stream() -> Vm {
    let mut vm = stage::sharing_window();
    let share = built(Share {
        switching: true,
        source: None,
        resolution: Resolution::P1080,
        fps: 30,
        ..fixtures::dialog()
    });
    let rows = own_menu(&share, true);
    vm.voice.share = share;
    vm.menu = Menu::new(1, Spot::pointer(Point::new(640.0, 260.0)), rows);
    vm
}

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "voice",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("golive_apps", || window(fixtures::dialog())),
        shot("golive_listed", || window(fixtures::listed())),
        shot("golive_one_screen", || window(fixtures::one_screen())),
        shot("golive_devices", || window(fixtures::devices())),
        shot("golive_no_devices", || window(fixtures::no_devices())),
        shot("golive_switching", || window(fixtures::switching())),
        shot("golive_gear", || with_gear(fixtures::dialog(), None)),
        shot("golive_gear_advanced", || {
            with_gear(fixtures::tuned(), Some(ADVANCED))
        }),
        // With Settings' "Unlocked bitrate": the steps to 50 Mbit/s and
        // "Custom value…", the limit typed by hand in the dialog's place,
        // refused, and the stream's menu saying a limit typed.
        shot("golive_gear_unlocked", || {
            with_gear(fixtures::unlocked(), Some(ADVANCED))
        }),
        shot("golive_bitrate_entry", || window(fixtures::typing())),
        shot("share_menu_live_quality", || {
            window(fixtures::live_quality())
        }),
        shot("share_menu_live_unlocked", || {
            window(fixtures::live_unlocked())
        }),
        shot("share_menu_live", || window(fixtures::live())),
        shot("own_stream_menu", own_stream),
    ]
}
