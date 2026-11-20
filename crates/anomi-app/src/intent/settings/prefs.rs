//! The switches of Messages, Notifications, Sounds, Discord, Advanced and
//! About (Game activity's are `super::games`'): each kept in the layout as
//! the Slint app keeps it (`on_preferences_changed`,
//! `src/ui/callbacks.rs:2303`; `updates.rs`) and handed on at once where the
//! Slint app hands it on: to the core, the clocks, the call's audio or the
//! desktop. A switch the rest
//! of the window shows (the clock, compact messages, the text's size) shows
//! there at once. Advanced's Optimization moves the program onto the cores
//! chosen, switches what draws the interface (`crate::gpu::choose`) and
//! tells the stream workers where to encode and whether their bitrate may
//! pass Discord's documented 9 Mbit/s, at once.

use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::chat::RelationshipChange;
use yapfast_core::sounds::{Sound, SoundSet};

use anomi_vm::settings::advanced::CpuMode;
use anomi_vm::settings::{about, advanced, discord, messages, notifications, sounds};

use crate::app::App;
use crate::layout::{Animate, CpuChoice, Encoder, Saved};
use crate::message::Message;
use crate::project::Reach;

/// The fork's own repository, and the original project's.
const GITHUB: &str = "https://github.com/Mrkdsjjsjs/yapfast";
const WEBSITE: &str = "https://github.com/yapfast/yapfast";

/// Keeps a change and shows Settings again: what nothing else shows.
fn keep(app: &mut App, change: impl FnOnce(&mut Saved)) -> Task<Message> {
    app.layout.edit(change);
    app.show(Reach::SETTINGS);
    Task::none()
}

/// Keeps a change the rest of the window shows too.
fn keep_everywhere(app: &mut App, change: impl FnOnce(&mut Saved)) -> Task<Message> {
    app.layout.edit(change);
    app.refresh();
    Task::none()
}

/// Keeps a change and hands the core the switch it acts on.
fn keep_and(app: &mut App, change: impl FnOnce(&mut Saved), action: Action) -> Task<Message> {
    app.layout.edit(change);
    app.dispatch(action)
}

/// Hands what formats times the date format and the clock, as the Slint
/// app's `apply_date_format`: Discord's timestamps in messages follow them.
pub(super) fn clock(saved: &Saved) {
    use yapfast_core::clock;
    let format = (saved.date_format).and_then(|index| {
        clock::DateFormat::ALL
            .get(usize::try_from(index).ok()?)
            .copied()
    });
    clock::set_date_format(format);
    clock::set_twelve_hour_clock(saved.twelve_hour_clock);
}

pub(super) fn messages(app: &mut App, intent: messages::Intent) -> Task<Message> {
    use messages::Intent as I;
    match intent {
        I::Compact(on) => keep_everywhere(app, |saved| saved.compact_messages = on),
        I::TextSize(size) => {
            let index = messages::TextSize::ALL.iter().position(|&one| one == size);
            keep_everywhere(app, |saved| saved.text_size = index.map(|at| at as i32))
        }
        I::TwelveHour(on) => {
            app.layout.edit(|saved| saved.twelve_hour_clock = on);
            clock(&app.layout);
            app.refresh();
            Task::none()
        }
        I::Seconds(on) => keep_everywhere(app, |saved| saved.clock_seconds = on),
        I::DateFormat(format) => {
            let index = messages::DateFormat::ALL
                .iter()
                .position(|&one| one == format);
            app.layout
                .edit(|saved| saved.date_format = index.map(|at| at as i32));
            clock(&app.layout);
            app.refresh();
            Task::none()
        }
        I::Animate(animate) => {
            let never = animate == messages::Animate::Never;
            app.motion.never = never;
            keep(app, |saved| {
                saved.animate = if never {
                    Animate::Never
                } else {
                    Animate::Focused
                };
            })
        }
        I::LinkPreviews(on) => keep_everywhere(app, |saved| saved.link_previews = Some(on)),
        I::CollapseLong(on) => keep_everywhere(app, |saved| saved.collapse_long = on),
        I::BoostGems(on) => keep_everywhere(app, |saved| saved.boost_gems = Some(on)),
        // The effects read it as each keystroke would tell Discord.
        I::ShareTyping(on) => keep(app, |saved| saved.share_typing = Some(on)),
        I::ReplyPing(on) => keep_and(
            app,
            |saved| saved.reply_ping = Some(on),
            Action::ReplyPing(on),
        ),
        I::StripTracking(on) => keep_and(
            app,
            |saved| saved.strip_tracking = Some(on),
            Action::StripTracking(on),
        ),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

pub(super) fn notifications(app: &mut App, intent: notifications::Intent) -> Task<Message> {
    use notifications::Intent as I;
    match intent {
        I::Notifications(on) => keep(app, |saved| saved.notifications = Some(on)),
        I::Text(on) => keep(app, |saved| saved.notification_text = Some(on)),
        I::FlashTaskbar(on) => keep(app, |saved| saved.flash_taskbar = Some(on)),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Plays a sound by its place in `Sound::ALL`, as it's set now.
fn play(app: &App, index: usize) {
    if let Some(&sound) = Sound::ALL.get(index) {
        crate::effects::sound::play(app, sound);
    }
}

pub(super) fn sounds(app: &mut App, intent: sounds::Intent) -> Task<Message> {
    use sounds::Intent as I;
    app.vm.settings.sounds.apply(&intent);
    match intent {
        I::Set(set) => {
            let set = SoundSet::ALL.get(set).copied().unwrap_or_default();
            app.layout.edit(|saved| saved.sound_set = set);
            play(app, 0);
        }
        // Under a drag the page shows the value; it's kept as it settles.
        I::Volume(_) | I::Each(..) => return Task::none(),
        I::VolumeSettled(volume) => {
            app.layout
                .edit(|saved| saved.sound_volume = Some(volume.clamp(0, 200) as u8));
            play(app, sounds::MESSAGE);
        }
        I::AdjustEach(on) => app.layout.edit(|saved| saved.adjust_each_sound = on),
        I::EachSettled(index, share) => {
            if let Some(&sound) = Sound::ALL.get(index) {
                let share = share.clamp(0, 100) as u8;
                app.layout.edit(|saved| {
                    if share == 100 {
                        saved.each_sound.remove(&sound);
                    } else {
                        saved.each_sound.insert(sound, share);
                    }
                });
                play(app, index);
            }
        }
        I::Play(index) => play(app, index),
        intent => tracing::debug!(?intent, "not handled by the iced interface yet"),
    }
    app.show(Reach::SETTINGS);
    Task::none()
}

pub(super) fn discord(app: &mut App, intent: discord::Intent) -> Task<Message> {
    use discord::Intent as I;
    match intent {
        I::ShowHiddenChannels(on) => keep_and(
            app,
            |saved| saved.show_hidden_channels = on,
            Action::ShowHiddenChannels(on),
        ),
        I::SyncReads(on) => keep_and(
            app,
            |saved| saved.sync_discord_reads = Some(on),
            Action::SyncDiscordReads(on),
        ),
        I::ShowIdle(on) => {
            app.layout.edit(|saved| saved.show_idle = Some(on));
            app.show(Reach::SETTINGS);
            crate::clocks::away::show_idle_changed(app)
        }
        I::HideBlocked(on) => keep_and(
            app,
            |saved| saved.hide_blocked = on,
            Action::HideBlocked(on),
        ),
        I::Unblock(user) => app.dispatch(Action::ChangeRelationship {
            user,
            change: RelationshipChange::Unblock,
            location: "ContextMenu",
        }),
        I::MoreBlocked => {
            app.vm.settings.discord.blocked_shown += discord::BLOCKED_STEP;
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not the Discord page's");
            Task::none()
        }
    }
}

pub(super) fn advanced(app: &mut App, intent: advanced::Intent) -> Task<Message> {
    use advanced::Intent as I;
    use anomi_desktop::{autostart, protocol};
    match intent {
        I::StartOnSignIn(on) if autostart::SUPPORTED && on != autostart::enabled() => {
            match autostart::set(on) {
                Ok(()) => tracing::info!(on, "starting at sign-in changed"),
                Err(error) => tracing::warn!(%error, "could not change starting at sign-in"),
            }
            app.shown.settings.kept.changed();
        }
        I::DiscordLinks(on) if cfg!(feature = "discord") && protocol::SUPPORTED => {
            match protocol::set_discord_enabled(&app.root, on) {
                Ok(()) => tracing::info!(on, "Discord link handling changed"),
                Err(error) => tracing::warn!(%error, "could not change Discord link handling"),
            }
            app.shown.settings.kept.changed();
        }
        I::CloseToTray(on) => app.layout.edit(|saved| saved.close_to_tray = Some(on)),
        I::MultipleWindows(on) => app.layout.edit(|saved| saved.multiple_windows = on),
        // The menus offer copying ids with it.
        I::DeveloperMode(on) => return keep_everywhere(app, |saved| saved.developer_mode = on),
        I::CpuMode(at) => {
            let choice = cpu_choice(&app.layout, CpuMode::at(at));
            cores_chosen(app, choice);
        }
        I::Core(core) => {
            if let CpuChoice::Cores(mut cores) = app.layout.cores() {
                match cores.iter().position(|&one| one == core) {
                    // One core at least: none would leave nowhere to run.
                    Some(_) if cores.len() == 1 => {}
                    Some(at) => {
                        cores.remove(at);
                    }
                    None => {
                        cores.push(core);
                        cores.sort_unstable();
                    }
                }
                cores_chosen(app, CpuChoice::Cores(cores));
            }
        }
        I::Encoder(key) if !key.is_empty() => {
            app.layout
                .edit(|saved| saved.set_encoder(&Encoder::of(&key)));
            #[cfg(feature = "discord")]
            yapfast_core::stream_worker::set_encode_on(app.layout.encoder().key());
        }
        // A stream going takes it at once; the share menus and the dock
        // say the limit in force.
        I::BitrateUnlocked(on) => {
            app.layout
                .edit(|saved| saved.stream_bitrate_unlocked = on);
            #[cfg(feature = "discord")]
            yapfast_core::stream_worker::unlock_bitrate(on);
            crate::intent::voice::share::unlocked(app, on);
            app.show(Reach::VOICE);
        }
        I::Renderer(key) if !key.is_empty() => {
            let choice = crate::gpu::Choice::parse(Some(key.as_str()));
            let switched = crate::gpu::choose(app, choice);
            app.show(Reach::SETTINGS);
            return switched;
        }
        intent => tracing::debug!(?intent, "nothing to change"),
    }
    app.show(Reach::SETTINGS);
    Task::none()
}

/// What choosing a way to spread over the cores keeps: choosing by hand
/// starts from the cores chosen before, or from those Auto would mostly
/// take.
fn cpu_choice(saved: &Saved, mode: CpuMode) -> CpuChoice {
    match (mode, saved.cores()) {
        (CpuMode::Auto, _) => CpuChoice::Auto,
        (CpuMode::All, _) => CpuChoice::All,
        (CpuMode::Chosen, CpuChoice::Cores(cores)) if !cores.is_empty() => CpuChoice::Cores(cores),
        // Where the processor's cores can't be read there are none to tick.
        (CpuMode::Chosen, _) => match crate::project::settings::pages::first_cores() {
            cores if cores.is_empty() => CpuChoice::Auto,
            cores => CpuChoice::Cores(cores),
        },
    }
}

/// Keeps the cores chosen and moves the program onto them at once, with
/// the streams it runs.
fn cores_chosen(app: &mut App, choice: CpuChoice) {
    if app.layout.cores() == choice && app.layout.cpu_mode.is_some() {
        return;
    }
    app.layout.edit(|saved| saved.cpu_mode = Some(choice));
    crate::run::place_on_cores(&app.layout);
}

pub(super) fn about(app: &mut App, intent: about::Intent) -> Task<Message> {
    use about::Intent as I;
    match intent {
        I::CopyVersion => return super::frame::copy(app),
        I::Check(on) => return keep(app, |saved| saved.check_for_updates = on),
        I::Nightlies(on) => return keep(app, |saved| saved.update_nightlies = on),
        I::OpenGithub => crate::effects::page::open(app, GITHUB.to_owned()),
        I::OpenWebsite => crate::effects::page::open(app, WEBSITE.to_owned()),
        // This fork has no releases to look for or install.
        intent => tracing::debug!(?intent, "no releases"),
    }
    Task::none()
}
