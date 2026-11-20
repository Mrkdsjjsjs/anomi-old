//! Going live, as Discord's client has it (the owner's screenshots,
//! `discord-golive-*.png`): the Go Live dialog (`dialog`) with its two
//! tabs, "Applications" (`apps`) and "Devices" (`devices`), the stream's
//! mode and quality under them, the button that starts it and the gear,
//! whose menu ([`gear`]) is the window's context menu.
//!
//! While live the same layer draws the stream's own menu instead
//! ([`live`]): its quality and sound in submenus, each choice changing the
//! stream at once, "Change Windows", which opens the dialog again to
//! switch, and "Stop Streaming". A right click on one's own stream's tile
//! opens Discord's menu of it ([`own_menu`]), the window's too.
//!
//! The shell draws the dialog for `Layer::Share` in the middle of the
//! window, and the stream's menu from the button that opened it. The picks
//! live in [`Share`], so a click asks and the app projects the dialog
//! again; the view ticks nothing and the screens' pictures are handles the
//! app made once.
//!
//! With Settings' "Unlocked bitrate", "Custom value…" among the limit's
//! steps opens the limit typed by hand ([`entry`]), which the same layer
//! draws in the middle of the window in place of the dialog or the menu,
//! until it's set or left.

mod apps;
mod card;
mod devices;
mod dialog;
pub mod entry;
pub mod live;
mod menus;
mod rows;
#[cfg(feature = "shots")]
mod shots;
mod tile;

use anomi_theme::prelude::*;
use anomi_vm::voice::share::{Intent, Share};

pub use dialog::{GO, HEIGHT, WIDTH};
pub use menus::{gear, own_menu};
pub use rows::{bitrate, built, quality, resolution};
#[cfg(feature = "shots")]
pub use shots::shots;

/// The limit typed by hand while it's open, else the dialog, or the
/// stream's own menu while live.
pub fn view(share: &Share, motion: Motion) -> Element<'_, Intent> {
    if let Some(typed) = &share.entry {
        return entry::view(typed, motion);
    }
    match &share.live {
        Some(stream) => live::view(stream, motion),
        None => dialog::view(share, motion),
    }
}
