//! The stream's menus that are the window's context menu: the Go Live
//! dialog's gear (`anomi_voice::share::gear`) and a right click on one's
//! own stream's tile (`anomi_voice::share::own_menu`). Their rows carry the
//! dialog's intents, which `picks` carries out; a box to tick and a radio
//! leave the menu open, so its rows are made again after each choice. The
//! gear's goes with its dialog; the tile's follows the stream, closing as
//! it ends.

use iced::Task;
use yapfast_core::voice::{SharePhase, StreamAudio};

use anomi_vm::menu::{Menu, Row, Spot};
use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{Share, Sound};

use crate::app::App;
use crate::message::Message;

/// Which of the stream's menus shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Which {
    Gear,
    /// One's own stream's tile's, "Change Windows" waiting until the
    /// stream is live.
    Own {
        changeable: bool,
    },
}

/// The gear's menu, above the gear.
pub(super) fn gear(app: &mut App, spot: Spot) -> Task<Message> {
    show(app, Which::Gear, spot);
    Task::none()
}

/// A right click on one's own stream's tile: its menu at the pointer, from
/// the stream's picks. The stream's own menu closes first, as the window
/// shows one menu at a time.
pub fn own_menu(app: &mut App, spot: Spot) -> Task<Message> {
    let phase = (app.core.state.voice.sharing.as_ref()).map(|sharing| sharing.phase);
    let Some(phase) = phase else {
        return Task::none();
    };
    let task = if app.vm.shell.is_open(Layer::Share) {
        crate::intent::voice::popups::close(app, Layer::Share)
    } else {
        Task::none()
    };
    let share = super::picks(app);
    app.vm.voice.share = anomi_voice::share::built(share);
    let changeable = phase == SharePhase::Live;
    show(app, Which::Own { changeable }, spot);
    task
}

fn rows(share: &Share, which: Which) -> Vec<Row> {
    match which {
        Which::Gear => anomi_voice::share::gear(share),
        Which::Own { changeable } => anomi_voice::share::own_menu(share, changeable),
    }
}

fn show(app: &mut App, which: Which, spot: Spot) {
    let rows = rows(&app.vm.voice.share, which);
    crate::intent::menus::show(app, Menu::new(0, spot, rows));
    app.shown.voice.share.menu = Some((app.vm.menu.serial, which));
}

/// The stream's menu showing, if the window's menu is one.
fn showing(app: &App) -> Option<Which> {
    let (serial, which) = app.shown.voice.share.menu?;
    (app.vm.menu.open && app.vm.menu.serial == serial).then_some(which)
}

/// Its rows again, from the picks as they are now.
pub(super) fn refresh(app: &mut App) {
    if let Some(which) = showing(app) {
        app.vm.menu.rows = rows(&app.vm.voice.share, which);
    }
}

/// The dialog closed: its gear's menu goes too.
pub(super) fn dialog_closed(app: &mut App) {
    if showing(app) == Some(Which::Gear) {
        crate::intent::menus::close(app);
    }
}

/// Either of the stream's menus closes, as when "Custom value…", a radio
/// row that would leave it open, opens the hand entry in its place.
pub(super) fn close(app: &mut App) {
    if showing(app).is_some() {
        crate::intent::menus::close(app);
    }
}

/// After each update: the tile's menu closes as the stream ends, and
/// follows whether it's live and the sound the dock's button turns off or
/// back on. A few comparisons while nothing changed.
pub(super) fn follow(app: &mut App) {
    let Some(which) = showing(app) else {
        app.shown.voice.share.menu = None;
        return;
    };
    let Which::Own { changeable } = which else {
        return;
    };
    let voice = &app.core.state.voice;
    let Some(sharing) = &voice.sharing else {
        crate::intent::menus::close(app);
        return;
    };
    let live = sharing.phase == SharePhase::Live;
    let silent = voice.stream_audio == StreamAudio::Off;
    let share = &mut app.vm.voice.share;
    if live == changeable && (share.sound == Sound::None) == silent {
        return;
    }
    share.sound = if silent { Sound::None } else { Sound::All };
    if let Some((_, which)) = app.shown.voice.share.menu.as_mut() {
        *which = Which::Own { changeable: live };
    }
    super::rebuild(app);
    refresh(app);
}
