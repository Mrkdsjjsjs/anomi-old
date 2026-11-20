//! Going live: the Go Live dialog, opened once what can be shared is listed
//! (`list_share_sources`, `src/ui/callbacks.rs:3104`), and its picks
//! (`picks`), which go live or switch what's shared (`go_live`,
//! `switch_live`, `stream_sound_changed`). Listing starts a capturer for
//! screens and one for windows and looks at each camera, so it runs on the
//! core's blocking pool; the dialog opens for the listing it waited for,
//! and a second click, Escape or leaving forgets it (upstream's 084240b6).
//! On a Wayland session the system's dialog picks the screen or window:
//! the dialog's cards ask it for a screen alone or a window alone, its
//! "Choose" for either. The screens' pictures come while the dialog shows
//! (`glance`). The gear's menu and the menu of one's own stream's tile are
//! the window's context menu (`menus`).
//!
//! While live the share buttons, and the gears on "Your screen" and on this
//! account's stream's tile, open the stream's own menu at once, as
//! Discord's client does: a choice of frame rate, resolution, encoding,
//! bitrate limit or sound changes the stream at once (`Command::Quality`,
//! which the Slint menu didn't offer) and the menu stays open; "Change
//! Windows" opens the dialog to pick what to switch to; "Stop Streaming"
//! ends the stream.
//!
//! With Settings' "Unlocked bitrate" the limit's steps go past 9 Mbit/s,
//! and "Custom value…" opens the limit typed by hand (`entry`) in place of
//! the dialog or the menu.

mod entry;
mod menus;
mod picks;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_bitrate;

use iced::Task;
use yapfast_core::stream_worker::{self, Shareable, Source};
use yapfast_core::voice::{self, SharePhase, StreamAudio};
use yapfast_lang::tr;

use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{Live, Resolution, Screen, Share, Sound};

use crate::app::App;
use crate::layout::{ENCODE_AUTO, ENCODE_SOFTWARE};
use crate::message::{Message, Once};

pub(crate) use entry::cancel as cancel_entry;
pub use menus::own_menu;
pub use picks::handle;

/// The listing the dialog waits for, what its sources are, and the stream's
/// menu that shows.
#[derive(Default)]
pub struct Shown {
    /// The listing waited for, by its number, and where the menu opens.
    waiting: Option<(u64, Spot)>,
    asked: u64,
    /// The sources in the dialog's order: screens, windows, cameras.
    pub(super) sources: Vec<Source>,
    /// The screens' pictures being taken for the dialog of that listing.
    glance: Option<(u64, iced::task::Handle)>,
    /// "Encode streams on"'s keys, in the order of the dialog's choices.
    encoder_keys: Vec<String>,
    /// The window's menu of the stream that shows, by its serial.
    menu: Option<(u64, menus::Which)>,
    /// The resolution and frame rate the person set before a preset took
    /// their place: "Custom" brings them back.
    custom: Option<(Resolution, u32)>,
    /// The share layer opened for the limit typed by hand alone, from the
    /// menu of one's own stream's tile: leaving it closes the layer.
    entry_alone: bool,
}

impl Shown {
    /// Whether the dialog of listing `asked` shows and takes its pictures.
    pub(super) fn glancing(&self, asked: u64) -> bool {
        self.glance
            .as_ref()
            .is_some_and(|(glancing, _)| *glancing == asked)
    }
}

/// What the listing found.
#[derive(Clone, Debug)]
pub struct Listed(pub u64, pub Once<Vec<Shareable>>);

/// The share button: while live, the stream's own menu at once; else what
/// can be shared is listed, then the dialog opens. A second click closes
/// it, or forgets the listing it waits for.
pub fn open(app: &mut App, spot: Spot) -> Task<Message> {
    let shown = &mut app.shown.voice.share;
    if shown.waiting.take().is_some() || app.vm.shell.is_open(Layer::Share) {
        return super::popups::close(app, Layer::Share);
    }
    let phase = (app.core.state.voice.sharing.as_ref()).map(|sharing| sharing.phase);
    if let Some(phase) = phase {
        let share = Share {
            live: Some(Live::new(phase == SharePhase::Live)),
            ..picks(app)
        };
        app.vm.voice.share = anomi_voice::share::built(share);
        app.vm.shell.open_at(Layer::Share, spot);
        return Task::none();
    }
    list(app, spot)
}

/// Lists what can be shared, for the dialog to open once it's in.
pub(super) fn list(app: &mut App, spot: Spot) -> Task<Message> {
    let shown = &mut app.shown.voice.share;
    shown.asked += 1;
    let asked = shown.asked;
    shown.waiting = Some((asked, spot));
    // Listed as the task runs, not before.
    let runtime = app.runtime.clone();
    let listing = async move { runtime.spawn_blocking(stream_worker::shareable).await };
    Task::perform(listing, move |listed| {
        Message::Voice(super::Due::Listed(Listed(
            asked,
            Once::new(listed.unwrap_or_default()),
        )))
    })
}

/// The listing came: the dialog opens, if it still waits for this one.
pub fn listed(app: &mut App, Listed(asked, listed): Listed) -> Task<Message> {
    let shown = &mut app.shown.voice.share;
    let Some((waited, spot)) = shown.waiting else {
        return Task::none();
    };
    let Some(listed) = listed.take().filter(|_| waited == asked) else {
        return Task::none();
    };
    shown.waiting = None;
    let (mut screens, mut windows, mut cameras) = (Vec::new(), Vec::new(), Vec::new());
    for listed in listed {
        match listed.source {
            Source::Display(_) => screens.push(listed),
            Source::Camera(_) => cameras.push(listed),
            _ => windows.push(listed),
        }
    }
    shown.sources = (screens.iter().chain(&windows).chain(&cameras))
        .map(|listed| listed.source.clone())
        .collect();
    let titles = |listed: &[Shareable]| -> Vec<String> {
        listed.iter().map(|listed| listed.title.clone()).collect()
    };
    // Where the system's dialog picks, nothing but cameras is listed.
    let dialog = screens.is_empty() && windows.is_empty() && stream_worker::portal_picks();
    let share = Share {
        screens: (screens.iter())
            .map(|listed| Screen {
                title: listed.title.clone(),
                picture: None,
            })
            .collect(),
        windows: titles(&windows),
        cameras: titles(&cameras),
        dialog,
        ..picks(app)
    };
    app.vm.voice.share = anomi_voice::share::built(share);
    app.vm.shell.open_at(Layer::Share, spot);
    let shown = &app.shown.voice.share;
    let Some((pictures, handle)) = super::glance::start(app, asked, &shown.sources) else {
        return Task::none();
    };
    app.shown.voice.share.glance = Some((asked, handle));
    pictures
}

/// The dialog's picks as the call and the layout have them: the stream's
/// quality and sound, its preview, where it encodes, and the bitrate's
/// unlock. While live nothing is picked, so Switch waits for a pick and a
/// change of sound or quality switches nothing (`open-share-menu`). The
/// limit is the one kept, past 9 Mbit/s too, which the menus show as 9
/// without the unlock and which goes back to the core as it was.
pub(super) fn picks(app: &mut App) -> Share {
    let (encoders, encoder) = encoders(app);
    let voice = &app.core.state.voice;
    Share {
        switching: voice.sharing.is_some(),
        source: voice.sharing.is_none().then_some(0),
        resolution: menu_resolution(voice.quality.resolution),
        fps: voice.quality.fps,
        high_quality: voice.quality.high_quality,
        max_kbps: voice.quality.max_kbps,
        unlocked: app.layout.stream_bitrate_unlocked,
        sound: match voice.stream_audio {
            StreamAudio::Off => Sound::None,
            _ => Sound::All,
        },
        preview_hidden: app.layout.hide_stream_preview,
        encoders,
        encoder,
        ..Share::default()
    }
}

/// "Encode streams on" as Settings offers it (`project::settings::pages`):
/// Auto, each graphics card this build encodes on, the processor, and the
/// one chosen. The keys stay here, for a choice to name its card. Read from
/// sysfs as the dialog opens, which loads no driver.
fn encoders(app: &mut App) -> (Vec<String>, usize) {
    let cards = yapfast_core::adapters::list();
    let mut names = vec![tr!("Auto").to_owned()];
    let mut keys = vec![ENCODE_AUTO.to_owned()];
    for card in cards.iter().filter(|card| card.encoder_api().is_some()) {
        names.push(card.name.clone());
        keys.push(card.key.clone());
    }
    names.push(tr!("Processor").to_owned());
    keys.push(ENCODE_SOFTWARE.to_owned());
    let chosen = app.layout.encoder();
    let at = keys.iter().position(|key| key == chosen.key()).unwrap_or(0);
    app.shown.voice.share.encoder_keys = keys;
    (names, at)
}

/// The menu's resolution for the core's.
pub(crate) fn menu_resolution(resolution: voice::Resolution) -> Resolution {
    match resolution {
        voice::Resolution::P720 => Resolution::P720,
        voice::Resolution::P1080 => Resolution::P1080,
        voice::Resolution::P1440 => Resolution::P1440,
        voice::Resolution::Source => Resolution::Source,
    }
}

/// The dialog closed: a listing still waited for is forgotten, and its
/// gear's menu goes with it. The limit typed by hand stays drawn while the
/// layer plays its way out, and goes as the layer opens again.
pub fn closed(app: &mut App) -> Task<Message> {
    app.shown.voice.share.waiting = None;
    app.shown.voice.share.glance = None;
    app.shown.voice.share.entry_alone = false;
    menus::dialog_closed(app);
    Task::none()
}

/// Settings' "Unlocked bitrate" changed: the dialog or the menu showing
/// offers the steps past 9 Mbit/s, or no more, and says the limit in force.
pub(crate) fn unlocked(app: &mut App, on: bool) {
    let share = &mut app.vm.voice.share;
    if share.unlocked == on {
        return;
    }
    share.unlocked = on;
    rebuild(app);
    menus::refresh(app);
}

/// After each update: the menus of a stream follow it. They close as it
/// ends; "Change Windows" waits until the stream is live, and the sound
/// follows the dock's button.
pub fn follow(app: &mut App) -> Task<Message> {
    menus::follow(app);
    let share = &app.vm.voice.share;
    if !share.switching || !app.vm.shell.is_open(Layer::Share) {
        return Task::none();
    }
    let voice = &app.core.state.voice;
    let Some(sharing) = &voice.sharing else {
        return super::popups::close(app, Layer::Share);
    };
    let Some(live) = &share.live else {
        return Task::none();
    };
    let changeable = sharing.phase == SharePhase::Live;
    let silent = voice.stream_audio == StreamAudio::Off;
    if live.changeable == changeable && (share.sound == Sound::None) == silent {
        return Task::none();
    }
    let share = &mut app.vm.voice.share;
    if let Some(live) = share.live.as_mut() {
        live.changeable = changeable;
    }
    share.sound = if silent { Sound::None } else { Sound::All };
    rebuild(app);
    Task::none()
}

/// The dialog's rows and text again, from its picks.
fn rebuild(app: &mut App) {
    let share = std::mem::take(&mut app.vm.voice.share);
    app.vm.voice.share = anomi_voice::share::built(share);
}
