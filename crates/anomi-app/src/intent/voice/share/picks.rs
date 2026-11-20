//! What the dialog and the stream's menus ask: a pick changes the dialog's
//! view-model and, while live, the stream at once; "Choose", "Stream" and
//! "Switch" carry the picks to the core. The stream's modes are presets
//! over the resolution and the frame rate (`anomi_vm::voice::share::Mode`):
//! a preset remembers what the person set, which "Custom" brings back.

use iced::Task;
use yapfast_core::stream_worker::{self, Source};
use yapfast_core::voice::{self, Command, StreamAudio, StreamQuality};

use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{Intent, Mode, Offer, Resolution, Share, Sound};

use super::{list, menus, rebuild};
use crate::app::App;
use crate::intent::voice::{command, popups};
use crate::layout::Encoder;
use crate::message::Message;
use crate::project::Reach;

/// What "Custom" brings back when the person set nothing of their own: the
/// core's own quality, 1080p at 30 frames.
const CUSTOM: (Resolution, u32) = (Resolution::P1080, 30);

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let share = &mut app.vm.voice.share;
    let retuned = matches!(
        intent,
        Intent::Mode(_)
            | Intent::Resolution(_)
            | Intent::Fps(_)
            | Intent::HighQuality(_)
            | Intent::Bitrate(_)
    );
    match intent {
        Intent::Hover(_) | Intent::Key(_) => {
            // A key that chose a row does as a click on it would.
            let chosen = (share.live.as_mut()).and_then(|live| live.apply(&intent));
            return match chosen {
                Some(chosen) => handle(app, chosen),
                None => Task::none(),
            };
        }
        Intent::Change => {
            let spot = (app.vm.shell.opened(Layer::Share)).and_then(|opened| opened.at);
            return list(app, spot.unwrap_or_default());
        }
        // The limit typed by hand, with the unlock.
        Intent::CustomBitrate => return super::entry::open(app),
        Intent::BitrateText(text) => {
            super::entry::edit(app, text);
            return Task::none();
        }
        Intent::ApplyBitrate => return super::entry::apply(app),
        Intent::CancelBitrate => return super::entry::cancel(app),
        Intent::Stop => {
            let task = popups::close(app, Layer::Share);
            return Task::batch([task, command(app, Command::StopLive)]);
        }
        Intent::Gear(spot) => return menus::gear(app, spot),
        Intent::Ask(offer) => return ask(app, offer),
        Intent::Tab(tab) => share.show(tab),
        Intent::Source(source) => share.source = Some(source),
        Intent::Mode(mode) => set_mode(&mut app.shown.voice.share.custom, share, mode),
        Intent::Resolution(resolution) => share.resolution = resolution,
        Intent::Fps(fps) => share.fps = fps,
        Intent::HighQuality(on) => share.high_quality = on,
        Intent::Bitrate(kbps) => share.max_kbps = kbps,
        Intent::HidePreview(hidden) => {
            share.preview_hidden = hidden;
            app.layout.edit(|saved| saved.hide_stream_preview = hidden);
            app.show(Reach::VOICE);
        }
        Intent::Encoder(at) => encoder(app, at),
        Intent::Sound(sound) => {
            share.sound = sound;
            let audio = match sound {
                Sound::None => StreamAudio::Off,
                Sound::All => StreamAudio::All,
            };
            if audio != StreamAudio::Off {
                app.shown.voice.heard = audio;
            }
            rebuild(app);
            menus::refresh(app);
            return command(app, Command::StreamAudio(audio));
        }
        Intent::GoLive {
            source,
            resolution,
            fps,
            high_quality,
        } => {
            pick(app, source, true);
            let limit = app.vm.voice.share.max_kbps;
            let quality = quality(resolution, fps, high_quality, limit);
            let task = popups::close(app, Layer::Share);
            return Task::batch([task, command(app, Command::GoLive(quality))]);
        }
        Intent::Switch(source) => {
            pick(app, source, false);
            let task = popups::close(app, Layer::Share);
            return Task::batch([task, command(app, Command::SwitchLive)]);
        }
        intent => tracing::debug!(?intent, "not the share dialog's"),
    }
    rebuild(app);
    menus::refresh(app);
    let share = &app.vm.voice.share;
    if retuned && share.switching {
        let quality = of(share);
        return command(app, Command::Quality(quality));
    }
    Task::none()
}

/// A stream mode chosen: a preset sets its resolution and frame rate,
/// remembering the person's own; "Custom" brings theirs back.
fn set_mode(custom: &mut Option<(Resolution, u32)>, share: &mut Share, mode: Mode) {
    let now = (share.resolution, share.fps);
    let next = match mode.preset() {
        Some(preset) => {
            if share.mode() == Mode::Custom {
                *custom = Some(now);
            }
            preset
        }
        None if share.mode() != Mode::Custom => custom.unwrap_or(CUSTOM),
        None => now,
    };
    (share.resolution, share.fps) = next;
}

/// Goes live, or switches while live, with what the system's dialog picks,
/// offering `offer`: a card asks it for a screen alone or a window alone,
/// "Choose" for either.
fn ask(app: &mut App, offer: Offer) -> Task<Message> {
    stream_worker::pick(match offer {
        Offer::Screen => Source::PortalScreen,
        Offer::Window => Source::PortalWindow,
        Offer::Either => Source::Screen,
    });
    let share = &app.vm.voice.share;
    let next = if share.switching {
        Command::SwitchLive
    } else {
        Command::GoLive(of(share))
    };
    let task = popups::close(app, Layer::Share);
    Task::batch([task, command(app, next)])
}

/// Streams encode on the choice at `at`, kept as Settings keeps it, and a
/// stream going takes it at once.
fn encoder(app: &mut App, at: usize) {
    let Some(key) = app.shown.voice.share.encoder_keys.get(at).cloned() else {
        return;
    };
    app.layout
        .edit(|saved| saved.set_encoder(&Encoder::of(&key)));
    stream_worker::set_encode_on(app.layout.encoder().key());
    app.vm.voice.share.encoder = at;
    app.show(Reach::SETTINGS);
}

/// The core's quality for the dialog's picks.
fn of(share: &Share) -> StreamQuality {
    quality(
        share.resolution,
        share.fps,
        share.high_quality,
        share.max_kbps,
    )
}

/// The core's quality for the menu's choices: the bitrate limit a ceiling
/// within Discord's, or past it with the unlock, `None` for Discord's
/// alone; kept as chosen, so a limit past 9 Mbit/s isn't lost while the
/// unlock is off.
fn quality(
    resolution: Resolution,
    fps: u32,
    high_quality: bool,
    max_kbps: Option<u32>,
) -> StreamQuality {
    StreamQuality {
        resolution: match resolution {
            Resolution::P720 => voice::Resolution::P720,
            Resolution::P1080 => voice::Resolution::P1080,
            Resolution::P1440 => voice::Resolution::P1440,
            Resolution::Source => voice::Resolution::Source,
        },
        fps: fps.clamp(1, 60),
        high_quality,
        max_kbps,
    }
}

/// What's shared next: the source at `index` of the dialog; going live
/// without one asks the system's dialog.
fn pick(app: &App, index: usize, live: bool) {
    match app.shown.voice.share.sources.get(index) {
        Some(source) => stream_worker::pick(source.clone()),
        None if live => stream_worker::pick(Source::Screen),
        None => {}
    }
}
