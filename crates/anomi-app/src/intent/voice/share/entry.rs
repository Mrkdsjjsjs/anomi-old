//! The bitrate limit typed by hand, with Settings' "Unlocked bitrate"
//! (`anomi_vm::voice::share::entry`). "Custom value…" among the limit's
//! steps opens it in the share layer, in place of the Go Live dialog or the
//! stream's menu, closing the window's menu it was chosen in; from the menu
//! of one's own stream's tile, which is the window's, the layer opens for
//! it alone. A value set is the limit, as a step chosen is, and changes a
//! stream going at once; one that isn't a limit stays, with why under it.
//! Leaving it, by "Cancel", Escape or a press beside it, brings back what
//! it came from, or closes the layer it opened.

use iced::Task;

use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{Entry, Intent, entry};

use crate::app::App;
use crate::intent::voice::popups;
use crate::message::Message;

/// "Custom value…": the field opens with the limit in force and takes the
/// keys. Nothing without the unlock.
pub(super) fn open(app: &mut App) -> Task<Message> {
    let share = &mut app.vm.voice.share;
    if !share.unlocked {
        return Task::none();
    }
    share.entry = Some(Entry::new(share.limit()));
    super::menus::close(app);
    let alone = !app.vm.shell.is_open(Layer::Share);
    app.shown.voice.share.entry_alone = alone;
    if alone {
        app.vm.shell.open(Layer::Share);
    }
    anomi_widgets::field::focus(anomi_voice::share::entry::FIELD)
}

/// The field was typed in.
pub(super) fn edit(app: &mut App, text: String) {
    if let Some(entry) = app.vm.voice.share.entry.as_mut() {
        entry.edit(text);
    }
}

/// Enter or "Apply": the value typed is the limit, if it's one; else the
/// field says why and stays.
pub(super) fn apply(app: &mut App) -> Task<Message> {
    let Some(typed) = app.vm.voice.share.entry.as_mut() else {
        return Task::none();
    };
    match entry::parse(&typed.text) {
        Ok(kbps) => {
            app.vm.voice.share.entry = None;
            let chosen = super::picks::handle(app, Intent::Bitrate(Some(kbps)));
            Task::batch([chosen, leave(app)])
        }
        Err(refusal) => {
            typed.refused = Some(refusal);
            Task::none()
        }
    }
}

/// "Cancel", Escape or a press beside it: the limit as it was.
pub fn cancel(app: &mut App) -> Task<Message> {
    app.vm.voice.share.entry = None;
    leave(app)
}

/// Back to what the entry came from: the layer it opened alone closes.
fn leave(app: &mut App) -> Task<Message> {
    if std::mem::take(&mut app.shown.voice.share.entry_alone) {
        popups::close(app, Layer::Share)
    } else {
        Task::none()
    }
}
