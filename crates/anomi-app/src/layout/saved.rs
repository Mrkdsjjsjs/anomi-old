//! What `layout.json` holds: the Slint app's `Layout` (`src/ui.rs:1287`),
//! field for field, with the same names, defaults and serde attributes, so
//! either app reads what the other wrote. A field whose type the Slint app
//! keeps in its own crate is held here as the JSON it is, untouched, until
//! the task that uses it gives it its type: the custom themes and fonts
//! (I19) and the window's placement (I14). Keys this
//! interface doesn't know at all, such as a newer Slint app's, stay in
//! [`Saved::other`] and go back into the file as they came.
//!
//! Anomi's own keys, which the Slint app doesn't have (the processor's
//! cores, what draws the interface, where streams encode and their
//! bitrate's unlock, echo cancellation, the people whose voice is cleaned
//! and the streams muted),
//! are written only once set, so a file the Slint app wrote reads and
//! writes back unchanged; it keeps them through its own catch-all.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use yapfast_core::keybinds::{Binding, Shortcut};
use yapfast_core::noise::{Strength, Suppressor};
use yapfast_core::sounds::{Sound, SoundSet};

use super::stickers::{KeptSticker, KeptStickerUse};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub sidebar_width: Option<f32>,
    /// The width of a Discord thread shown beside its channel.
    pub aside_width: Option<f32>,
    /// The member list's width, as its edge was last dragged; none for the
    /// style's own.
    pub members_width: Option<f32>,
    /// The height of a DM's call above its messages, as its lower edge was
    /// last dragged; none for the proportions of the interface.
    pub call_height: Option<f32>,
    /// The expression picker's width, as its left edge last dragged it
    /// wider. `gif_picker_width` named it while only the GIFs took it.
    #[serde(alias = "gif_picker_width")]
    pub picker_width: Option<f32>,
    /// The member list shows until someone closes it.
    pub members_hidden: bool,
    /// So does the profile beside a one-to-one DM.
    pub profile_panel_hidden: bool,
    /// How far the whole window is zoomed, as one of the zoom's levels; 100%
    /// unless changed.
    pub zoom: Option<f32>,
    /// Each server's last selected channel, by server ID.
    pub channels: HashMap<String, String>,
    /// Each server's collapsed channel categories, by server ID: a Discord
    /// category by its room id, any other by its heading.
    pub collapsed: HashMap<String, BTreeSet<String>>,
    /// The rail's open folders, by folder ID.
    pub open_folders: BTreeSet<String>,
    /// How often this device added each reaction, for the frequent reactions.
    pub reactions: HashMap<String, u32>,
    /// The stickers this device sent, and how often, for Frequently Used.
    pub sticker_uses: Vec<KeptStickerUse>,
    /// The stickers this device keeps, for Favorites.
    pub sticker_favorites: Vec<KeptSticker>,
    /// List Discord channels hidden from this account, locked.
    pub show_hidden_channels: bool,
    /// Reading here marks Discord channels read on Discord; on unless off.
    pub sync_discord_reads: Option<bool>,
    /// Discord shows this account idle after 10 minutes unused; on unless
    /// turned off.
    pub show_idle: Option<bool>,
    /// Games this device names itself: an executable, by file name or full
    /// path, and the game to show for it.
    pub game_executables: Vec<(String, String)>,
    /// Games added from Discord's list (`super::games`), Anomi's own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub discord_games: Vec<super::games::DiscordGame>,
    /// Messages from people this account blocked don't show at all.
    pub hide_blocked: bool,
    /// A call from someone's menu starts without asking first.
    pub skip_call_prompt: bool,
    /// The member list's Activity panel shows only its header.
    pub activity_cards_hidden: bool,
    /// A message that notifies while the window isn't in front flashes the
    /// taskbar; on unless turned off.
    pub flash_taskbar: Option<bool>,
    /// DMs pinned to the top of the DM list, by room ID, the first first.
    pub pinned: Vec<String>,
    /// The interface's language, by its code in `yapfast_lang::LANGUAGES`.
    pub language: Option<String>,
    /// Message times as 9:41 PM rather than 21:41.
    pub twelve_hour_clock: bool,
    /// Message times with their seconds, as 21:41:05.
    pub clock_seconds: bool,
    /// The order of day, month and year, as an index into
    /// `yapfast_core::clock::DateFormat::ALL`; follows the clock unless chosen.
    pub date_format: Option<i32>,
    /// Message text: 0 small, 1 default, 2 large.
    pub text_size: Option<i32>,
    /// Messages as Discord's compact display shows them.
    pub compact_messages: bool,
    /// Link previews under messages; on unless turned off.
    pub link_previews: Option<bool>,
    /// A gem by each server booster in the member list; on unless off.
    pub boost_gems: Option<bool>,
    /// Replies mention the person they reply to; on unless turned off.
    pub reply_ping: Option<bool>,
    /// Tracking comes out of links in sent messages; on unless turned off.
    pub strip_tracking: Option<bool>,
    /// Nameplates show; on unless turned off.
    pub nameplates: Option<bool>,
    /// Long messages fold to a few lines, with Show more; off unless on.
    pub collapse_long: bool,
    /// When animations play.
    pub animate: Animate,
    /// The profile switches, each as `Cosmetics::default` starts it unless
    /// changed.
    pub profile_effects: Option<bool>,
    pub decorations: Option<bool>,
    pub moving_cosmetics: Option<bool>,
    pub name_styles: Option<bool>,
    pub server_tags: Option<bool>,
    pub profile_frames: Option<bool>,
    /// Joining a call turns the microphone on; on unless turned off.
    pub unmute_on_join: Option<bool>,
    /// Joining a call deafens.
    pub deafen_on_join: bool,
    /// How many minutes a private Discord call waits with no one else in it
    /// before leaving it, zero to stay.
    pub alone_timeout: Option<u32>,
    /// Your voice is evened out in calls; on unless turned off.
    pub level_voice: Option<bool>,
    /// What takes out noise in Discord calls, and how far.
    pub noise_suppression: Suppressor,
    pub noise_strength: Strength,
    /// Discord calls take the room's reverb out of the microphone.
    pub reduce_room_echo: bool,
    /// Discord calls cancel the echo of what the speakers play; on unless
    /// turned off. Written only once set, as the Slint app doesn't know it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo_cancellation: Option<bool>,
    /// Windows runs its microphone effects on Discord calls' microphone.
    pub windows_microphone_effects: bool,
    /// The quietest speech Discord calls send, in dBFS; -60 unless changed.
    pub input_sensitivity: Option<i32>,
    /// Whether calls send what the gate hears or only while keys are held,
    /// the keys, and how long the microphone sends after they come up, in
    /// ms; Discord's 20 unless changed.
    pub input_mode: InputMode,
    pub push_to_talk_key: Option<Shortcut>,
    pub push_to_talk_release_delay: Option<u32>,
    /// The Keybinds page's keybinds, in its order.
    pub keybinds: Vec<Binding>,
    /// How loud every call sends the microphone and plays, as sliders'
    /// percentages from 0% to 200%; 100% unless changed.
    pub microphone_volume: Option<u8>,
    pub speaker_volume: Option<u8>,
    /// How loud people sound in calls, and their streams, by user id.
    pub volumes: BTreeMap<String, f32>,
    pub stream_volumes: BTreeMap<String, f32>,
    /// The streams muted here from the menu of their tile, by streamer: the
    /// core plays them at nothing, their volume above kept for when they're
    /// unmuted. Written once one is.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub muted_streams: BTreeSet<String>,
    /// Those muted for this account alone, from their menu in a call.
    pub muted_people: BTreeSet<String>,
    /// Those whose soundboard sounds this account doesn't hear.
    pub soundboard_muted_people: BTreeSet<String>,
    /// How this device cleans each person's voice, by user id, from their
    /// menu in a call (`super::cleaning`); written once someone is in it.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub cleaned_voices: BTreeMap<String, super::CleanedVoice>,
    /// The same for the sound of each one's stream watched, by streamer.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub cleaned_streams: BTreeMap<String, super::CleanedVoice>,
    /// Others see when this account is typing; on unless turned off.
    pub share_typing: Option<bool>,
    /// Desktop notifications, and whether they say what messages say; on
    /// unless turned off.
    pub notifications: Option<bool>,
    pub notification_text: Option<bool>,
    /// Whether notifications played the desktop's sound, before the app had
    /// its own: read once, by [`Saved::settle`], never written.
    #[serde(skip_serializing)]
    pub notification_sound: Option<bool>,
    /// The instrument the app's sounds play on, and how loud they are as a
    /// slider's percentage from 0% to 200%.
    pub sound_set: SoundSet,
    pub sound_volume: Option<u8>,
    /// Each sound has its own volume too, against that one.
    pub adjust_each_sound: bool,
    /// Each sound's own volume as a percentage up to 100%, if it isn't 100%.
    pub each_sound: BTreeMap<Sound, u8>,
    /// The Slint app draws with Skia GPU from its next start. Anomi keeps it
    /// as it came and chooses with `interface_renderer`.
    pub gpu_rendering: bool,
    /// What draws Anomi's interface: unset or `"auto"` for Automatic (the
    /// integrated graphics, else the processor), `"processor"`, or a
    /// graphics adapter's key from `yapfast_core::adapters`
    /// (`"pci:0000:73:00.0"`), as `crate::gpu::Choice::parse` reads it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_renderer: Option<String>,
    /// How many times in a row the graphics adapter failed to start or was
    /// lost; `crate::gpu` counts them and turns to the processor after
    /// three.
    #[serde(skip_serializing_if = "is_zero")]
    pub renderer_failures: u8,
    /// The graphics adapter a window opened on, by its key: written before
    /// the window opens, cleared once it has drawn there a few seconds
    /// (`crate::gpu::opening`). Found at a start, the run before died on
    /// it, and the processor draws until a GPU is chosen again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer_trying: Option<String>,
    /// Screen sharing encodes on the GPU where it can; on unless turned off.
    /// Anomi chooses with `stream_encoder` and keeps this beside it for the
    /// Slint app: off for the processor, on for anything else.
    pub gpu_encoding: Option<bool>,
    /// Where streams encode: [`ENCODE_AUTO`], [`ENCODE_SOFTWARE`] or a
    /// graphics card's key from `yapfast_core::adapters`
    /// (`"pci:0000:73:00.0"`); unset, as `gpu_encoding` says
    /// ([`Saved::encoder`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_encoder: Option<String>,
    /// Go Live's "Hide stream preview" (its own stream's); written once set.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hide_stream_preview: bool,
    /// Settings' "Unlocked bitrate": a stream's bitrate limit may pass
    /// Discord's documented 9 Mbit/s, up to 50; written once set.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream_bitrate_unlocked: bool,
    /// The processor's cores the program runs on; Auto unless chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_mode: Option<CpuChoice>,
    /// The theme chosen, by its id: a built-in one's or `custom:N`.
    pub theme: Option<String>,
    /// The person's own themes, in the order Appearance shows them
    /// (`theme::Custom`).
    pub custom_themes: Vec<Value>,
    /// Closing the window leaves the app in the tray; on unless turned off.
    pub close_to_tray: Option<bool>,
    /// Starting the app again on this folder starts another.
    pub multiple_windows: bool,
    /// Menus offer Copy ID, as Discord's Developer Mode does.
    pub developer_mode: bool,
    /// About asks GitHub for the releases at the start; off unless on.
    pub check_for_updates: bool,
    /// Nightly builds count as newer releases.
    pub update_nightlies: bool,
    /// The tag of the release that Skip this version hid.
    pub skipped_update: Option<String>,
    /// The tag of the release a notification last told of.
    pub announced_update: Option<String>,
    /// The version whose settings this person counts as having met; see the
    /// Slint app's `new_tags`.
    pub first_version: Option<String>,
    /// The NEW settings this person has looked at.
    pub seen_new: Vec<String>,
    /// The desktop draws the window's title bar; unset, the app draws its
    /// own on desktops that give windows one.
    pub system_title_bar: Option<bool>,
    /// The font files the person loaded to draw the interface in
    /// (`custom_font::Stored`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ui_fonts: Vec<Value>,
    /// Where the window was when the app last ran, and how big
    /// (`placement::Placement`).
    pub window: Option<Value>,
    /// Every key neither app version above knows, kept as it came.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Saved {
    /// How long the microphone sends after the push-to-talk key comes up,
    /// in ms (`release_delay`, `src/ui.rs`).
    pub fn release_delay(&self) -> u32 {
        use yapfast_core::push_to_talk::{DEFAULT_RELEASE_DELAY, MOST_RELEASE_DELAY};
        (self.push_to_talk_release_delay)
            .unwrap_or(DEFAULT_RELEASE_DELAY)
            .min(MOST_RELEASE_DELAY)
    }

    /// Where streams encode: `stream_encoder`, or, before Anomi kept one,
    /// the processor where `gpu_encoding` was turned off.
    pub fn encoder(&self) -> Encoder {
        match self.stream_encoder.as_deref() {
            Some(key) if !key.is_empty() => Encoder::of(key),
            _ if self.gpu_encoding == Some(false) => Encoder::Software,
            _ => Encoder::Auto,
        }
    }

    /// Keeps where streams encode, and `gpu_encoding` beside it, which is
    /// what the Slint app reads.
    pub fn set_encoder(&mut self, encoder: &Encoder) {
        self.stream_encoder = Some(encoder.key().to_owned());
        self.gpu_encoding = Some(*encoder != Encoder::Software);
    }

    /// The processor's cores the program runs on, as chosen.
    pub fn cores(&self) -> CpuChoice {
        self.cpu_mode.clone().unwrap_or_default()
    }

    /// What the Slint app's `Layout::read` settles as it reads: a person who
    /// turned off the desktop's sound for notifications, before the app had
    /// its own sounds, keeps messages and mentions quiet; and Anomi's own
    /// voices cleaned as an earlier Anomi kept them (`super::cleaning`).
    pub fn settle(&mut self) {
        if self.notification_sound.take() == Some(false) {
            self.adjust_each_sound = true;
            self.each_sound.insert(Sound::Message, 0);
            self.each_sound.insert(Sound::Mention, 0);
        }
        self.settle_cleaning();
    }
}

/// A count that is written only once something counted.
fn is_zero(count: &u8) -> bool {
    *count == 0
}

/// `stream_encoder`'s keys for Auto and for the processor.
pub const ENCODE_AUTO: &str = "auto";
pub const ENCODE_SOFTWARE: &str = "software";

/// Where streams encode (`yapfast_core::stream_worker::EncodeOn`): as Auto
/// chooses, on a graphics card by its key, or on the processor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Encoder {
    #[default]
    Auto,
    Card(String),
    Software,
}

impl Encoder {
    /// From its key in `layout.json`: anything but Auto's and the
    /// processor's names a card, which Auto stands in for while it's gone.
    pub fn of(key: &str) -> Self {
        match key {
            ENCODE_AUTO => Self::Auto,
            ENCODE_SOFTWARE => Self::Software,
            card => Self::Card(card.to_owned()),
        }
    }

    /// Its key in `layout.json`.
    pub fn key(&self) -> &str {
        match self {
            Self::Auto => ENCODE_AUTO,
            Self::Software => ENCODE_SOFTWARE,
            Self::Card(key) => key.as_str(),
        }
    }
}

/// The processor's cores the program runs on (`yapfast_core::cpu::CpuMode`),
/// as `layout.json` keeps it: `"auto"`, `"all"`, or the physical cores
/// chosen, `{"cores": [8, 9, …]}`. The mode is kept, never a mask: Auto is
/// worked out again on each start, on whatever processor it is then.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CpuChoice {
    #[default]
    Auto,
    All,
    Cores(Vec<u32>),
}

/// How calls decide when to send the microphone, as Discord's Input Mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    #[default]
    VoiceActivity,
    PushToTalk,
}

/// When GIFs, animated emoji and stickers play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Animate {
    /// While the window has the focus. A layout that chose Always reads as
    /// this: nothing moves in a window that isn't in front.
    #[default]
    #[serde(alias = "always")]
    Focused,
    Never,
}
