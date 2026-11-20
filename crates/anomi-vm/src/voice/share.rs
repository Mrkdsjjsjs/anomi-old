//! Going live: Discord's Go Live dialog and the stream's own menus, what
//! they show and what they ask for. Task V10 (docs/iced/tasks/voice.md)
//! began it after Slint's `ShareMenu` (`ui/voice.slint:1414`); the owner's
//! screenshots of Discord's dialog (`discord-golive-*.png`) shape it now.
//!
//! The dialog has two tabs. "Applications" shares a screen or a window:
//! where the system's dialog picks one (Wayland's portal) it offers
//! Discord's two cards, the whole screen and a window, each asking that
//! dialog for its kind alone, and "Choose" asks it for either; elsewhere it
//! lists the screens and the windows to pick from. "Devices" lists the
//! cameras and capture cards. Under them, the stream's mode and quality
//! ("Custom Quality", "720p · 60fps · System sound"), the button that starts the
//! stream, and the gear, whose menu (the window's context menu, built by
//! `anomi_voice::share::gear`) holds the modes, the resolution, the frame
//! rate, the sound and, under "Advanced", the preview, the bitrate limit
//! and the encoding.
//!
//! The picks live here, so a click changes them in the app and the dialog
//! is projected again; "Choose", "Start Streaming" and "Switch" carry them to the
//! core. While live the share button opens the stream's own menu instead
//! ([`Live`]); a right click on one's own stream's tile opens Discord's
//! menu of it (`anomi_voice::share::own_menu`). "Change Windows" opens the
//! dialog again, to switch.
//!
//! With Settings' "Unlocked bitrate" the limit's steps go past Discord's
//! documented 9 Mbit/s, and "Custom value…" opens a small dialog in the
//! same layer, in place of the dialog or the menu it came from, to type a
//! limit from 1 to 50 Mbit/s ([`entry`]).

pub mod entry;
mod live;

use iced::widget::image;

use crate::menu::{Key, Row, Spot};

pub use entry::Entry;
pub use live::{Live, Part};

/// A screen in the dialog, and once it's been glanced at, a picture of it
/// (`ShareScreen`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screen {
    pub title: String,
    /// `None` until the picture comes.
    pub picture: Option<image::Handle>,
}

/// The size a stream goes out at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Resolution {
    P720,
    #[default]
    P1080,
    /// 2560 by 1440, the "Games" mode's.
    P1440,
    /// As the source is.
    Source,
}

impl Resolution {
    pub const ALL: [Self; 4] = [Self::P720, Self::P1080, Self::P1440, Self::Source];
}

/// The frame rates offered.
pub const FRAME_RATES: [u32; 3] = [15, 30, 60];

/// The bitrate limits offered after Auto, in kbit/s, up to the 9 Mbit/s
/// Discord takes above 720p, 1440p's too. A limit, not a target: the stream
/// still goes lower as the network asks.
pub const BITRATES: [u32; 6] = [2_000, 3_500, 5_000, 6_000, 8_000, 9_000];
/// The steps past them that the unlock adds, in kbit/s.
pub const UNLOCKED_BITRATES: [u32; 5] = [12_000, 15_000, 20_000, 30_000, 50_000];
/// The most a limit counts as without the unlock, in kbit/s: the core's
/// `voice::DOCUMENTED_KBPS`.
pub const DOCUMENTED_KBPS: u32 = 9_000;
/// The most it may be with the unlock: the core's `voice::UNLOCKED_KBPS`.
pub const UNLOCKED_KBPS: u32 = 50_000;

/// The sound shared: a choice changes it at once, live or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sound {
    /// Every application's but the client's own.
    #[default]
    All,
    /// "Mute stream audio": none.
    None,
}

/// The dialog's tabs, as Discord's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    /// A screen or a window.
    #[default]
    Apps,
    /// A camera or a capture card.
    Devices,
}

/// Discord's stream modes, presets over the resolution and the frame rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Smoother video: 1440p at 60 frames, as Discord's.
    Games,
    /// Sharper text: the screen's own size at 15 frames, as Discord's.
    Screen,
    /// What the person set.
    Custom,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Games, Self::Screen, Self::Custom];

    /// The resolution and frame rate a preset sets; none for Custom.
    pub fn preset(self) -> Option<(Resolution, u32)> {
        match self {
            Self::Games => Some((Resolution::P1440, 60)),
            Self::Screen => Some((Resolution::Source, 15)),
            Self::Custom => None,
        }
    }

    /// The mode a quality is: a preset's own, else Custom.
    pub fn of(resolution: Resolution, fps: u32) -> Self {
        (Self::ALL.into_iter())
            .find(|mode| mode.preset() == Some((resolution, fps)))
            .unwrap_or(Self::Custom)
    }
}

/// What the system's dialog offers when it picks what to share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    Screen,
    Window,
    /// Screens and windows, as "Choose" asks.
    Either,
}

/// The Go Live dialog: what its screen draws. The window holds it as
/// `vm.voice.share`.
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub screens: Vec<Screen>,
    pub windows: Vec<String>,
    pub cameras: Vec<String>,
    /// The system's dialog picks the screen or window (Wayland's portal):
    /// "Applications" offers its two kinds rather than a list.
    pub dialog: bool,
    /// Live: the button switches what's shared, and a choice of sound or
    /// quality changes the stream at once.
    pub switching: bool,
    pub tab: Tab,
    /// What's picked to share: an index in screens, windows and cameras,
    /// counted in that order. `None` while live, until something to switch
    /// to is picked.
    pub source: Option<usize>,
    pub resolution: Resolution,
    pub fps: u32,
    /// The encoder's slower preset: a sharper picture for more CPU.
    pub high_quality: bool,
    /// The most the stream sends, in kbit/s; `None` for Auto, as Discord
    /// allows at the size streamed. Kept as chosen: past 9 Mbit/s it counts
    /// as 9 while `unlocked` is off ([`Share::limit`]).
    pub max_kbps: Option<u32>,
    /// Settings' "Unlocked bitrate": the limit's steps go past 9 Mbit/s,
    /// and one can be typed by hand.
    pub unlocked: bool,
    /// The limit being typed by hand, shown in place of the dialog or the
    /// menu, while it is.
    pub entry: Option<Entry>,
    pub sound: Sound,
    /// "Hide stream preview": one's own stream's tile shows no picture of
    /// it, and none is made for it.
    pub preview_hidden: bool,
    /// "Encode streams on", as Settings offers it: each choice's words
    /// (Auto, each graphics card, the processor), and the one chosen.
    pub encoders: Vec<String>,
    pub encoder: usize,
    /// Text and rows made from the picks by `anomi_voice::share::built`
    /// (translated there): a view borrows them. The projection and a shot
    /// call it after setting the rest.
    pub rows: Rows,
    /// While live, the stream's own menu, drawn instead of the dialog;
    /// `built` makes its rows from the picks above.
    pub live: Option<Live>,
}

/// What `built` makes for the dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    /// Each window to pick, where the system's dialog doesn't.
    pub windows: Vec<Row<Intent>>,
    /// The mode's name at the dialog's bottom left ("Custom Quality").
    pub mode: String,
    /// The quality and the sound under it ("720p · 60fps · System sound").
    pub quality: String,
}

impl Default for Share {
    fn default() -> Self {
        Self {
            screens: Vec::new(),
            windows: Vec::new(),
            cameras: Vec::new(),
            dialog: false,
            switching: false,
            tab: Tab::default(),
            source: Some(0),
            resolution: Resolution::default(),
            fps: 30,
            high_quality: false,
            max_kbps: None,
            unlocked: false,
            entry: None,
            sound: Sound::default(),
            preview_hidden: false,
            encoders: Vec::new(),
            encoder: 0,
            rows: Rows::default(),
            live: None,
        }
    }
}

impl Share {
    /// Where the first camera sits among the sources.
    pub fn first_camera(&self) -> usize {
        self.screens.len() + self.windows.len()
    }

    /// Whether the source at `at` is one the tab shown lists.
    pub fn in_tab(&self, at: usize) -> bool {
        let cameras = self.first_camera();
        match self.tab {
            Tab::Apps => at < cameras,
            Tab::Devices => (cameras..cameras + self.cameras.len()).contains(&at),
        }
    }

    /// The stream's mode, from its resolution and frame rate.
    pub fn mode(&self) -> Mode {
        Mode::of(self.resolution, self.fps)
    }

    /// The bitrate limit in force, in kbit/s, none for Auto: the one kept,
    /// no more than 9 Mbit/s without the unlock, nor 50 with it.
    pub fn limit(&self) -> Option<u32> {
        let most = if self.unlocked {
            UNLOCKED_KBPS
        } else {
            DOCUMENTED_KBPS
        };
        self.max_kbps.map(|kbps| kbps.min(most))
    }

    /// The limit's steps after Auto: past 9 Mbit/s too with the unlock.
    pub fn steps(&self) -> Vec<u32> {
        let past: &[u32] = if self.unlocked {
            &UNLOCKED_BITRATES
        } else {
            &[]
        };
        BITRATES.iter().chain(past).copied().collect()
    }

    /// Shows `tab`, picking the first it lists where the pick was the other
    /// tab's; while live nothing is picked for a switch until a click.
    pub fn show(&mut self, tab: Tab) {
        self.tab = tab;
        if self.source.is_some_and(|at| self.in_tab(at)) {
            return;
        }
        let first = match tab {
            Tab::Apps => 0,
            Tab::Devices => self.first_camera(),
        };
        self.source = (!self.switching && self.in_tab(first)).then_some(first);
    }

    /// What the button at the bottom right sends, or `None` while it waits
    /// for a pick: where the system's dialog picks, "Choose" asks it for a
    /// screen or a window; else the source picked in the tab shown goes
    /// live, or replaces what's shared.
    pub fn go(&self) -> Option<Intent> {
        if self.tab == Tab::Apps && self.dialog {
            return Some(Intent::Ask(Offer::Either));
        }
        let source = self.source.filter(|&at| self.in_tab(at))?;
        Some(if self.switching {
            Intent::Switch(source)
        } else {
            Intent::GoLive {
                source,
                resolution: self.resolution,
                fps: self.fps,
                high_quality: self.high_quality,
            }
        })
    }

    pub fn can_go(&self) -> bool {
        self.go().is_some()
    }
}

/// The dialog and the stream's menus: what a click or a key on them asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Pick what to share, by its index among the sources.
    Source(usize),
    Tab(Tab),
    /// A stream mode: a preset sets its resolution and frame rate; Custom
    /// brings back what the person set before.
    Mode(Mode),
    Resolution(Resolution),
    Fps(u32),
    HighQuality(bool),
    /// Choose the sound; the app applies it at once, as
    /// `stream_sound_changed` does, and keeps the choice.
    Sound(Sound),
    /// A bitrate limit, in kbit/s, or Auto; while live it changes the
    /// stream at once.
    Bitrate(Option<u32>),
    /// "Custom value…", with the unlock: the limit typed by hand
    /// ([`Entry`]) shows in place of the dialog or the menu.
    CustomBitrate,
    /// The limit's field was typed in: its whole text.
    BitrateText(String),
    /// Enter in the field, or its "Apply": the value typed is the limit, if
    /// it's one ([`entry::parse`]); else the field says why.
    ApplyBitrate,
    /// Escape, a click beside it, or "Cancel": back to the dialog or the
    /// menu, the limit as it was.
    CancelBitrate,
    /// Hide one's own stream's preview, or show it again.
    HidePreview(bool),
    /// Encode streams on the choice at this index of `Share::encoders`;
    /// a stream going takes it at once.
    Encoder(usize),
    /// The gear: its menu opens there.
    Gear(Spot),
    /// Go live, or switch while live, with what the system's dialog picks,
    /// offering this; the dialog closes.
    Ask(Offer),
    /// Start the stream: the source, the quality and the encoder's preset
    /// (`go_live`), with the bitrate limit picked; the dialog closes.
    GoLive {
        source: usize,
        resolution: Resolution,
        fps: u32,
        high_quality: bool,
    },
    /// While live, share what's picked instead (`switch_live`); the dialog
    /// closes.
    Switch(usize),
    /// The pointer came onto a row of the stream's menu, or a click on one:
    /// it lights and its submenu opens, closing another ([`Live::apply`]).
    Hover(Option<Part>),
    /// A key, while the stream's menu has them.
    Key(Key),
    /// "Change Windows": the dialog opens again to pick what to switch to.
    Change,
    /// "Stop Streaming": the stream ends and the menu closes.
    Stop,
}

#[cfg(test)]
mod tests;
