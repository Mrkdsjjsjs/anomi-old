//! The pages that show the layout alone: Messages, Notifications, Sounds,
//! Advanced and About, as the Slint app's `Layout::apply` sets their
//! properties (`src/ui.rs:1728`) and `src/ui/updates.rs` fills About. Each
//! is built again only after the layout changed. Advanced's Optimization
//! also reads the processor and the graphics cards from the system then,
//! and what draws the interface (`super::drawing`); its bitrate's unlock is
//! the layout's.

use anomi_vm::settings::{about, advanced, messages, notifications, sounds};
use yapfast_core::new_tags;
use yapfast_core::sounds::{Sound, SoundSet};
use yapfast_lang::{PRODUCT, tr};

use super::Kept;
use crate::layout::{Animate, CpuChoice, ENCODE_AUTO, ENCODE_SOFTWARE, Encoder, Layout};

/// The choices Settings offers for how long an empty private call waits, in
/// minutes, in its order (`WAITS`, `src/ui/alone.rs`), and the one until
/// another is chosen.
pub(crate) const WAITS: [u32; 5] = [0, 3, 10, 30, 60];
const WAIT: usize = 1;

/// The choice of [`WAITS`] the saved wait is, or the one nearest it.
pub(crate) fn alone_choice(layout: &Layout) -> usize {
    match layout.alone_timeout {
        None => WAIT,
        Some(0) => 0,
        Some(saved) => (1..WAITS.len())
            .min_by_key(|&choice| WAITS[choice].abs_diff(saved))
            .unwrap_or(WAIT),
    }
}

/// Sets `page` to `built` when they differ.
fn set<T: PartialEq>(page: &mut T, built: T) {
    if *page != built {
        *page = built;
    }
}

pub(super) fn messages(layout: &Layout, page: &mut messages::Messages) {
    use messages::{Animate as Shown, DateFormat, TextSize};
    let date_format = (layout.date_format)
        .and_then(|index| DateFormat::ALL.get(usize::try_from(index).ok()?).copied());
    let built = messages::Messages {
        compact: layout.compact_messages,
        text_size: match layout.text_size {
            Some(0) => TextSize::Small,
            Some(2) => TextSize::Large,
            _ => TextSize::Default,
        },
        twelve_hour: layout.twelve_hour_clock,
        seconds: layout.clock_seconds,
        date_format,
        animate: match layout.animate {
            Animate::Focused => Shown::Focused,
            Animate::Never => Shown::Never,
        },
        link_previews: layout.link_previews.unwrap_or(true),
        collapse_long: layout.collapse_long,
        boost_gems: layout.boost_gems.unwrap_or(true),
        share_typing: layout.share_typing.unwrap_or(true),
        reply_ping: layout.reply_ping.unwrap_or(true),
        strip_tracking: layout.strip_tracking.unwrap_or(true),
    };
    set(page, built);
}

pub(super) fn notifications(layout: &Layout, page: &mut notifications::Notifications) {
    let built = notifications::Notifications {
        notifications: layout.notifications.unwrap_or(true),
        text: layout.notification_text.unwrap_or(true),
        flash_taskbar: layout.flash_taskbar.unwrap_or(true),
    };
    set(page, built);
}

pub(super) fn sounds(layout: &Layout, page: &mut sounds::Sounds) {
    let set_at = SoundSet::ALL
        .iter()
        .position(|&set| set == layout.sound_set);
    let built = sounds::Sounds {
        set: set_at.unwrap_or(0),
        volume: layout.sound_volume.map_or(100, i32::from),
        adjust_each: layout.adjust_each_sound,
        each: Sound::ALL.map(|sound| layout.each_sound.get(&sound).map_or(100, |&v| v.into())),
    };
    set(page, built);
}

/// Advanced, with Optimization: the processor and the graphics cards are
/// read from the system as the page is built, which is as it opens and
/// after a change (about 2 ms), never while it only shows.
pub(super) fn advanced(layout: &Layout, kept: &Kept, page: &mut advanced::Advanced) {
    use anomi_desktop::{autostart, protocol};
    let links = cfg!(feature = "discord") && protocol::SUPPORTED;
    let (cpu_line, core_groups) = processor(layout);
    let cards = yapfast_core::adapters::list();
    let drawing = super::drawing::drawing(layout, &cards);
    let encoding = encoding(layout, &cards);
    let mode = match layout.cores() {
        CpuChoice::Auto => advanced::CpuMode::Auto,
        CpuChoice::All => advanced::CpuMode::All,
        CpuChoice::Cores(_) => advanced::CpuMode::Chosen,
    };
    let built = advanced::Advanced {
        close_to_tray: layout.close_to_tray.unwrap_or(true),
        multiple_windows: layout.multiple_windows,
        developer_mode: layout.developer_mode,
        start_on_sign_in: autostart::SUPPORTED && autostart::enabled(),
        discord_links: links && protocol::discord_enabled(layout.folder()),
        tray_available: kept.tray,
        autostart_available: autostart::SUPPORTED,
        discord_links_available: links,
        cpu_mode: mode.index(),
        cpu_line,
        core_groups,
        renderers: drawing.rows,
        renderer: drawing.chosen,
        renderer_line: drawing.line,
        encoders: encoding.names,
        encoder_keys: encoding.keys,
        encoder: encoding.chosen,
        encoder_line: encoding.line,
        bitrate_unlocked: layout.stream_bitrate_unlocked,
    };
    set(page, built);
}

/// A logical CPU as the page needs it: its physical core's number, its L3
/// domain (a chiplet), that domain's size in KiB, and whether it's an
/// efficiency core.
type Placed = (u32, u32, u32, bool);

/// The processor's logical CPUs, as [`Placed`].
fn placed(topology: &yapfast_core::cpu::Topology) -> impl Iterator<Item = Placed> + '_ {
    (topology.cpus.iter()).map(|cpu| (cpu.core, cpu.l3, cpu.l3_kib, cpu.small))
}

/// What the processor is and which of its cores the program runs on, and
/// its physical cores to tick, by chiplet.
fn processor(layout: &Layout) -> (String, Vec<advanced::CoreGroup>) {
    use yapfast_core::cpu;
    let chosen = layout.cores();
    let topology = cpu::detect();
    let mask = cpu::choose(&topology, &crate::run::cpu_mode(&chosen));
    let line = cpu::describe(&topology, mask.as_ref());
    let ticked: &[u32] = match &chosen {
        CpuChoice::Cores(cores) => cores,
        _ => &[],
    };
    (line, core_groups(placed(&topology), ticked))
}

/// The physical cores by chiplet, the efficiency cores apart, each once,
/// ticked when in `ticked`.
fn core_groups(cpus: impl IntoIterator<Item = Placed>, ticked: &[u32]) -> Vec<advanced::CoreGroup> {
    use std::collections::{BTreeMap, BTreeSet};
    // A core's second thread names the same core.
    let mut cores: BTreeMap<u32, (u32, u32, bool)> = BTreeMap::new();
    for (core, l3, kib, small) in cpus {
        cores.entry(core).or_insert((l3, kib, small));
    }
    let domains: Vec<u32> = (cores.values().map(|(l3, ..)| *l3))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let hybrid = cores.values().any(|kind| kind.2) && cores.values().any(|kind| !kind.2);
    let title = |l3: u32, kib: u32, small: bool| -> String {
        let chiplet = domains.iter().position(|&one| one == l3).unwrap_or(0) + 1;
        match (small, hybrid, domains.len() > 1) {
            (true, ..) => tr!("Efficiency cores").to_owned(),
            (false, _, true) if kib >= 1024 => {
                tr!("Chiplet {} · L3 {} MB", chiplet, kib / 1024)
            }
            (false, _, true) => tr!("Chiplet {}", chiplet),
            (false, true, false) => tr!("Performance cores").to_owned(),
            (false, false, false) => tr!("Cores").to_owned(),
        }
    };
    let mut groups: Vec<((u32, bool), advanced::CoreGroup)> = Vec::new();
    for (id, (l3, kib, small)) in cores {
        let at = match groups.iter().position(|(key, _)| *key == (l3, small)) {
            Some(at) => at,
            None => {
                let group = advanced::CoreGroup {
                    title: title(l3, kib, small),
                    cores: Vec::new(),
                };
                groups.push(((l3, small), group));
                groups.len() - 1
            }
        };
        groups[at].1.cores.push(advanced::Core {
            id,
            label: id.to_string(),
            chosen: ticked.contains(&id),
        });
    }
    groups.sort_by_key(|(key, _)| *key);
    groups.into_iter().map(|(_, group)| group).collect()
}

/// The cores ticked as choosing them by hand begins, much as Auto takes
/// them: the efficiency cores where there are, else the last chiplet's
/// without the first core; every core where that leaves none.
pub(crate) fn first_cores() -> Vec<u32> {
    let topology = yapfast_core::cpu::detect();
    first_of(placed(&topology))
}

fn first_of(cpus: impl IntoIterator<Item = Placed>) -> Vec<u32> {
    use std::collections::BTreeMap;
    let cores: BTreeMap<u32, (u32, bool)> = (cpus.into_iter())
        .map(|(core, l3, _, small)| (core, (l3, small)))
        .collect();
    let small: Vec<u32> = (cores.iter())
        .filter(|(_, (_, small))| *small)
        .map(|(core, _)| *core)
        .collect();
    if !small.is_empty() {
        return small;
    }
    let last = cores.values().map(|(l3, _)| *l3).max();
    let picked: Vec<u32> = (cores.iter())
        .filter(|(core, (l3, _))| **core != 0 && Some(*l3) == last)
        .map(|(core, _)| *core)
        .collect();
    if picked.is_empty() {
        cores.keys().copied().collect()
    } else {
        picked
    }
}

/// "Encode streams on", as the page offers it.
struct Encoding {
    names: Vec<String>,
    keys: Vec<String>,
    chosen: usize,
    line: String,
}

/// Auto, each graphics card by name and the processor, the one chosen, and
/// what that does now: what Auto takes, or that the card chosen is gone.
fn encoding(layout: &Layout, cards: &[yapfast_core::adapters::Adapter]) -> Encoding {
    use yapfast_core::adapters;
    let auto = adapters::auto_encoder(cards, adapters::machine());
    let mut names = vec![tr!("Auto").to_owned()];
    let mut keys = vec![ENCODE_AUTO.to_owned()];
    // Only the cards this build encodes on are choices.
    for card in cards.iter().filter(|card| card.encoder_api().is_some()) {
        names.push(card.name.clone());
        keys.push(card.key.clone());
    }
    names.push(tr!("Processor").to_owned());
    keys.push(ENCODE_SOFTWARE.to_owned());
    let chosen = layout.encoder();
    let found = keys.iter().position(|key| key == chosen.key());
    let auto = auto.map_or_else(|| tr!("the processor").to_owned(), |card| card.name.clone());
    let line = match (&chosen, found) {
        (Encoder::Software, _) => tr!(
            "The processor encodes with x264: it takes processor time, and H.265 and AV1 aren't offered."
        )
        .to_owned(),
        (Encoder::Card(_), Some(_)) => String::new(),
        (Encoder::Card(_), None) => tr!(
            "The graphics card chosen isn't found now, so streams encode as Auto chooses: {}.",
            auto
        ),
        (Encoder::Auto, _) => tr!(
            "Auto takes the integrated graphics first, then a discrete card, then the processor. Now it's {}.",
            auto
        ),
    };
    Encoding {
        names,
        keys,
        chosen: found.unwrap_or(0),
        line,
    }
}

/// About: the build, the fork's name and the original credited; this fork
/// has no releases, so the update's switches are kept and nothing checks.
pub(super) fn about(layout: &Layout, page: &mut about::About) {
    let new = |key| new_tags::key_new(layout.first_version.as_deref(), &layout.seen_new, key);
    let built = about::About {
        build: crate::build_info::current(),
        copied: page.copied,
        product: PRODUCT.to_owned(),
        original: tr!("Original: {}", "Yapfast"),
        releases: false,
        manager: String::new(),
        status: about::Status::Idle,
        check: layout.check_for_updates,
        nightlies: layout.update_nightlies,
        fresh_check: new("check-updates"),
        fresh_nightlies: new("update-nightlies"),
        ..about::About::default()
    };
    set(page, built);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This repository's owner's 9950X3D: cores 0-7 on the chiplet with
    /// 96 MB of L3, 8-15 on the one with 32 MB, each core's second thread
    /// 16 CPUs on.
    fn x3d() -> Vec<Placed> {
        (0..32)
            .map(|cpu| {
                let core = cpu % 16;
                let (l3, kib) = if core < 8 { (0, 98_304) } else { (1, 32_768) };
                (core, l3, kib, false)
            })
            .collect()
    }

    /// A 13900K: eight performance cores, sixteen efficiency cores, one L3.
    fn raptor_lake() -> Vec<Placed> {
        (0..24).map(|core| (core, 0, 36_864, core >= 8)).collect()
    }

    #[test]
    fn cores_group_by_chiplet_each_once() {
        let groups = core_groups(x3d(), &[8, 9]);
        let titles: Vec<&str> = groups.iter().map(|group| group.title.as_str()).collect();
        assert_eq!(titles, ["Chiplet 1 · L3 96 MB", "Chiplet 2 · L3 32 MB"]);
        assert_eq!(groups[0].cores.len(), 8, "a second thread isn't a core");
        let ticked: Vec<u32> = (groups.iter())
            .flat_map(|group| &group.cores)
            .filter(|core| core.chosen)
            .map(|core| core.id)
            .collect();
        assert_eq!(ticked, [8, 9]);
        assert_eq!(groups[1].cores[0].label, "8");
    }

    #[test]
    fn efficiency_cores_are_a_group_of_their_own() {
        let groups = core_groups(raptor_lake(), &[]);
        let titles: Vec<&str> = groups.iter().map(|group| group.title.as_str()).collect();
        assert_eq!(titles, ["Performance cores", "Efficiency cores"]);
        assert_eq!(groups[1].cores.len(), 16);
        let plain = core_groups((0..8).map(|core| (core, 0, 32_768, false)), &[]);
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].title, "Cores");
    }

    #[test]
    fn choosing_by_hand_starts_near_auto() {
        assert_eq!(first_of(x3d()), (8..16).collect::<Vec<_>>());
        assert_eq!(first_of(raptor_lake()), (8..24).collect::<Vec<_>>());
        let one = (0..8).map(|core| (core, 0, 32_768, false));
        assert_eq!(
            first_of(one),
            (1..8).collect::<Vec<_>>(),
            "all but the first"
        );
        assert_eq!(first_of([(0, 0, 0, false)]), [0], "a core alone stays");
        assert!(first_of([]).is_empty());
    }
}
