//! What the dialog and the stream's menu say, made from the picks with the
//! text translated: the windows to pick, the mode and quality along the
//! dialog's bottom, and while live the stream's menu's rows, its submenus
//! with radio marks under the groups' titles, as Discord's client has them.
//! A row's view borrows its row, so [`built`] stores them in the view-model
//! (`Share::rows`, `Share::live`); the projection calls it after setting
//! the rest, and so do the shots and tests.

use yapfast_lang::tr;

use anomi_vm::menu::Row;
use anomi_vm::voice::share::{FRAME_RATES, Intent, Live, Mode, Part, Resolution, Rows, Share, Sound};

/// What the menus call a resolution.
pub fn resolution(resolution: Resolution) -> &'static str {
    match resolution {
        Resolution::P720 => "720p",
        Resolution::P1080 => "1080p",
        Resolution::P1440 => "1440p",
        Resolution::Source => tr!("Source"),
    }
}

/// What the menus call a bitrate limit, in kbit/s; Auto for none. Discord's
/// steps are whole texts; any other is its Mbit/s, with its decimals after
/// the mark each language writes ("12 Mbit/s", "20.5 Mbit/s").
pub fn bitrate(kbps: Option<u32>) -> String {
    match kbps {
        None => tr!("Auto").to_owned(),
        Some(2_000) => tr!("2 Mbit/s").to_owned(),
        Some(3_500) => tr!("3.5 Mbit/s").to_owned(),
        Some(5_000) => tr!("5 Mbit/s").to_owned(),
        Some(6_000) => tr!("6 Mbit/s").to_owned(),
        Some(8_000) => tr!("8 Mbit/s").to_owned(),
        Some(9_000) => tr!("9 Mbit/s").to_owned(),
        Some(kbps) if kbps.is_multiple_of(1000) => tr!("{} Mbit/s", kbps / 1000),
        Some(kbps) => {
            let decimals = format!("{:03}", kbps % 1000);
            tr!(
                "{0}.{1} Mbit/s",
                kbps / 1000,
                decimals.trim_end_matches('0')
            )
        }
    }
}

/// The limit the summaries say: none without the unlock, as before it, nor
/// for Auto.
fn said_limit(share: &Share) -> Option<u32> {
    share.limit().filter(|_| share.unlocked)
}

/// The limits, Auto first with what it sends, as radio rows; with the
/// unlock, the steps past 9 Mbit/s too, then "Custom value…", chosen while
/// the limit is none of the steps.
pub(super) fn limits<I>(share: &Share, intent: impl Fn(Intent) -> I) -> Vec<Row<I>> {
    let steps = share.steps();
    let limit = share.limit();
    let mut rows: Vec<Row<I>> = std::iter::once(None)
        .chain(steps.iter().copied().map(Some))
        .map(|kbps| {
            let row = Row::new(bitrate(kbps), intent(Intent::Bitrate(kbps)));
            let row = if kbps.is_none() {
                row.subtitle(tr!("3.5 Mbit/s up to 720p, 9 above"))
            } else {
                row
            };
            row.radio(limit == kbps).plain()
        })
        .collect();
    if share.unlocked {
        let custom = limit.filter(|kbps| !steps.contains(kbps));
        let said = custom.map_or_else(
            || tr!("From 1 to 50 Mbit/s").to_owned(),
            |kbps| bitrate(Some(kbps)),
        );
        rows.push(
            Row::new(tr!("Custom value…"), intent(Intent::CustomBitrate))
                .subtitle(said)
                .radio(custom.is_some())
                .plain(),
        );
    }
    rows
}

/// A stream mode's name.
pub(super) fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Games => tr!("Games"),
        Mode::Screen => tr!("Screen Share"),
        Mode::Custom => tr!("Custom Quality"),
    }
}

/// `share` with its rows made from its picks.
pub fn built(mut share: Share) -> Share {
    let windows = (share.windows.iter().enumerate())
        .map(|(at, title)| {
            let index = share.screens.len() + at;
            Row::new(title.as_str(), Intent::Source(index)).checked(share.source == Some(index))
        })
        .collect();
    let sound = match share.sound {
        Sound::All => tr!("System sound"),
        Sound::None => tr!("No sound"),
    };
    let size = resolution(share.resolution);
    let rate = tr!("{}fps", share.fps);
    // With the unlock, the limit chosen goes between the rate and the sound.
    let quality = match said_limit(&share) {
        Some(kbps) => format!("{size} · {rate} · {} · {sound}", bitrate(Some(kbps))),
        None => format!("{size} · {rate} · {sound}"),
    };
    share.rows = Rows {
        windows,
        mode: mode_name(share.mode()).to_owned(),
        quality,
    };
    if let Some(mut live) = share.live.take() {
        stream(&share, &mut live);
        share.live = Some(live);
    }
    share
}

/// What the stream goes out at, as the stream's menu and the dock say it:
/// "1080p · 60 fps"; with the unlock, the limit chosen after it, `limit`
/// in kbit/s: "1440p · 60 fps · 20 Mbit/s".
pub fn quality(resolution: Resolution, fps: u32, limit: Option<u32>) -> String {
    let said = tr!("{} · {} fps", self::resolution(resolution), fps);
    match limit {
        Some(kbps) => format!("{said} · {}", bitrate(Some(kbps))),
        None => said,
    }
}

/// The stream's menu's rows, from the picks: "Stream Quality" with the
/// frame rates, the resolutions, the encoding and the bitrate limit beside
/// it, "Sound" with its choices, "Change Windows" and "Stop Streaming".
fn stream(share: &Share, live: &mut Live) {
    let mut rates = vec![Row::heading(tr!("FRAME RATE"))];
    rates.extend(FRAME_RATES.into_iter().map(|fps| {
        Row::new(tr!("{} fps", fps), Intent::Fps(fps))
            .radio(share.fps == fps)
            .plain()
    }));
    rates.push(Row::heading(tr!("RESOLUTION")).separated());
    rates.extend(Resolution::ALL.into_iter().map(|choice| {
        Row::new(resolution(choice), Intent::Resolution(choice))
            .radio(share.resolution == choice)
            .plain()
    }));
    rates.push(Row::heading(tr!("ENCODING")).separated());
    rates.push(
        Row::new(
            tr!("Higher quality"),
            Intent::HighQuality(!share.high_quality),
        )
        .subtitle(tr!("Sharper, at twice the CPU"))
        .checkbox(share.high_quality)
        .plain(),
    );
    rates.push(Row::heading(tr!("BITRATE LIMIT")).separated());
    rates.extend(limits(share, |intent| intent));
    let choice = |text: &str, sound: Sound| {
        Row::new(text, Intent::Sound(sound))
            .radio(share.sound == sound)
            .plain()
    };
    let sounds = vec![
        choice(tr!("All applications"), Sound::All).subtitle(tr!("Except Yapfast's own sound")),
        choice(tr!("No sound"), Sound::None),
    ];
    let heard = match share.sound {
        Sound::All => tr!("All applications"),
        Sound::None => tr!("No sound"),
    };
    let opens = |part| Intent::Hover(Some(part));
    live.quality = Row::new(tr!("Stream Quality"), opens(Part::Quality))
        .subtitle(quality(share.resolution, share.fps, said_limit(share)))
        .submenu(rates)
        .plain();
    live.sound = Row::new(tr!("ShareMenu" => "Sound"), opens(Part::Sound))
        .subtitle(heard)
        .submenu(sounds)
        .plain();
    live.change = Row::new(tr!("Change Windows"), Intent::Change)
        .disabled(!live.changeable)
        .plain();
    live.stop = Row::new(tr!("Stop Streaming"), Intent::Stop)
        .danger()
        .plain();
    live.settle();
}
