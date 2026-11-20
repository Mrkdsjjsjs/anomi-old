//! Samples of the Advanced settings page for tests and shots, after the fake
//! Discord's data: task S10's, and Optimization on this repository's
//! owner's machine, a Ryzen 9 9950X3D with a Radeon 610M and an RTX 3080,
//! the monitor on the 3080, Vulkan's driver for the Radeon not installed.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::advanced::{Advanced, Core, CoreGroup, CpuMode, DrawOn};
use crate::settings::frame::Page;

/// The two chiplets' physical cores, the second's ticked.
fn chiplets() -> Vec<CoreGroup> {
    let group = |title: String, cores: std::ops::Range<u32>, chosen: bool| CoreGroup {
        title,
        cores: cores
            .map(|id| Core {
                id,
                label: id.to_string(),
                chosen,
            })
            .collect(),
    };
    vec![
        group(tr!("Chiplet {} · L3 {} MB", 1, 96), 0..8, false),
        group(tr!("Chiplet {} · L3 {} MB", 2, 32), 8..16, true),
    ]
}

/// The Radeon's name, as the system's PCI id list gives it.
const RADEON: &str = "AMD Radeon 610M";

/// "Interface rendering"'s rows, in PCI order as the adapters are listed;
/// the Radeon draws once its Vulkan driver is installed (`radeon_draws`).
fn renderers(radeon_draws: bool) -> Vec<DrawOn> {
    let row = |title: &str, description: String, key: &str, usable: bool| DrawOn {
        title: title.to_owned(),
        description,
        key: key.to_owned(),
        usable,
    };
    let (automatic, radeon) = if radeon_draws {
        (
            tr!(
                "The integrated graphics, else the processor. Here that's {}.",
                RADEON
            ),
            tr!("Integrated graphics.").to_owned(),
        )
    } else {
        (
            tr!(
                "The integrated graphics, else the processor. Here that's the processor, until {} can draw.",
                RADEON
            ),
            format!(
                "{} {}",
                tr!("Integrated graphics."),
                tr!(
                    "Its Vulkan driver isn't installed: it needs Mesa's Vulkan driver for Radeon (RADV)."
                )
            ),
        )
    };
    let processor = tr!(
        "The graphics cards stay free and the least memory is used, but a large window redraws slower."
    );
    let nvidia = tr!("A discrete card, which games run on, with the monitor connected to it.");
    vec![
        row(tr!("Automatic"), automatic, "auto", true),
        row(tr!("Processor"), processor.to_owned(), "processor", true),
        row(
            "NVIDIA GeForce RTX 3080",
            nvidia.to_owned(),
            "pci:0000:01:00.0",
            true,
        ),
        row(RADEON, radeon, "pci:0000:73:00.0", radeon_draws),
    ]
}

/// The golden screenshot's: no system tray, link handling and autostart
/// offered; Auto on the second chiplet and on the integrated graphics, and
/// Automatic drawing the interface on the processor, as the Radeon can't.
pub fn advanced() -> Advanced {
    Advanced {
        close_to_tray: true,
        multiple_windows: false,
        developer_mode: false,
        start_on_sign_in: false,
        discord_links: false,
        tray_available: false,
        autostart_available: true,
        discord_links_available: true,
        cpu_mode: CpuMode::Auto.index(),
        // As `yapfast_core::cpu::describe` says it.
        cpu_line: format!(
            "{} {}",
            tr!("Chiplets: {0}, cores: {1}, threads: {2}.", 2, 16, 32),
            tr!(
                "Runs on cores {0} (CPUs {1}): the chiplet with the smaller cache, {2} MB.",
                "8-15",
                "8-15, 24-31",
                32
            )
        ),
        core_groups: chiplets(),
        renderers: renderers(false),
        renderer: 0,
        renderer_line: format!(
            "{} {}",
            tr!("The processor draws the interface now."),
            tr!("No integrated graphics can draw the interface.")
        ),
        encoders: vec![
            tr!("Auto").into(),
            "NVIDIA GeForce RTX 3080".into(),
            "AMD Radeon 610M".into(),
            tr!("Processor").into(),
        ],
        // In PCI order, as the system lists them.
        encoder_keys: vec![
            "auto".into(),
            "pci:0000:01:00.0".into(),
            "pci:0000:73:00.0".into(),
            "software".into(),
        ],
        encoder: 0,
        encoder_line: tr!(
            "Auto takes the integrated graphics first, then a discrete card, then the processor. Now it's {}.",
            "AMD Radeon 610M"
        ),
        bitrate_unlocked: false,
    }
}

/// The window with the settings open on Advanced.
pub fn page() -> Vm {
    let mut vm = settings(Page::Advanced);
    vm.settings.advanced = advanced();
    vm
}

/// The cores chosen by hand, shown to tick; streams on the processor.
pub fn cores() -> Vm {
    let mut vm = page();
    let page = &mut vm.settings.advanced;
    page.cpu_mode = CpuMode::Chosen.index();
    page.encoder = 3;
    page.encoder_line = tr!(
        "The processor encodes with x264: it takes processor time, and H.265 and AV1 aren't offered."
    )
    .into();
    vm
}

/// A tray, but no autostart and no link handling: those switches go.
pub fn bare() -> Vm {
    let mut vm = page();
    let page = &mut vm.settings.advanced;
    page.tray_available = true;
    page.autostart_available = false;
    page.discord_links_available = false;
    vm
}

/// With the Radeon's Vulkan driver installed, Automatic draws on it; the
/// switches the system lacks are left out, so the rows show in the window.
pub fn gpu() -> Vm {
    let mut vm = bare();
    let page = &mut vm.settings.advanced;
    page.renderers = renderers(true);
    page.renderer_line = tr!("{} draws the interface now.", RADEON);
    vm
}

/// "Unlocked bitrate" on, the switches the system lacks left out, so the
/// row and its note show in the window.
pub fn unlocked() -> Vm {
    let mut vm = bare();
    vm.settings.advanced.bitrate_unlocked = true;
    vm
}

/// The 3080 chosen by hand, and the processor drawing after it failed.
pub fn fell_back() -> Vm {
    let mut vm = bare();
    let page = &mut vm.settings.advanced;
    page.renderer = 2;
    page.renderer_line = format!(
        "{} {}",
        tr!("The processor draws the interface now."),
        tr!(
            "The graphics adapter failed three times in a row, so the processor draws until it's chosen again."
        )
    );
    vm
}
