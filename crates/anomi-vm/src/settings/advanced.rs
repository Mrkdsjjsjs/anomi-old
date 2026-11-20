//! The Advanced settings page: what it shows and what it asks for. Task S10
//! (docs/iced/tasks/settings.md). The Slint side: `AdvancedSettings`
//! (`ui/settings.slint:1881`). Anomi adds Optimization under the switches:
//! the processor's cores the program runs on, what draws the interface,
//! where streams encode, and whether a stream's bitrate may pass Discord's
//! documented 9 Mbit/s.

/// The Advanced settings page: what its screen draws. The window holds it as
/// `vm.settings.advanced`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Advanced {
    /// What the switches hold.
    pub close_to_tray: bool,
    pub multiple_windows: bool,
    pub developer_mode: bool,
    pub start_on_sign_in: bool,
    pub discord_links: bool,
    /// A system tray is running: closing keeps the program in it.
    pub tray_available: bool,
    /// This system can start the program at sign-in; the switch shows only then.
    pub autostart_available: bool,
    /// This system can hand `discord://` links to the program.
    pub discord_links_available: bool,
    /// How the program spreads over the processor, by its place in
    /// [`CpuMode::ALL`].
    pub cpu_mode: usize,
    /// What the processor is and which of its cores the program runs on, in
    /// a line (`yapfast_core::cpu::describe`).
    pub cpu_line: String,
    /// The physical cores to tick, by chiplet (or by kind, on a processor
    /// with performance and efficiency cores); shown while choosing.
    pub core_groups: Vec<CoreGroup>,
    /// "Interface rendering": its rows in order (Automatic, the processor,
    /// each graphics adapter), which is chosen, and what draws the interface
    /// now, with why the processor does where an adapter was asked for.
    pub renderers: Vec<DrawOn>,
    pub renderer: usize,
    pub renderer_line: String,
    /// "Encode streams on": the choices' words in order (Auto, each graphics
    /// card by name, the processor), the key each sends, which is chosen,
    /// and what that choice does now, such as the card Auto chose.
    pub encoders: Vec<String>,
    pub encoder_keys: Vec<String>,
    pub encoder: usize,
    pub encoder_line: String,
    /// "Unlocked bitrate": a stream's bitrate limit may be set by hand past
    /// Discord's documented 9 Mbit/s, up to 50.
    pub bitrate_unlocked: bool,
}

/// A row of "Interface rendering": what may draw the interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawOn {
    /// "Automatic", "Processor", or the adapter's name.
    pub title: String,
    /// What it is on this computer: what Automatic takes here; whether an
    /// adapter is integrated or discrete and drives the monitor; why one
    /// that can't draw can't.
    pub description: String,
    /// What choosing it sends ([`Intent::Renderer`]): `layout.json`'s
    /// `interface_renderer` for it, `"auto"`, `"processor"` or the
    /// adapter's id (`"pci:0000:73:00.0"`).
    pub key: String,
    /// It can draw the window; a row that can't is faded and answers no
    /// click.
    pub usable: bool,
}

/// The processor's cores a page offers to choose from: a chiplet's, or the
/// efficiency cores, under its title.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoreGroup {
    pub title: String,
    pub cores: Vec<Core>,
}

/// A physical core, by its number, ticked while the program may run on it.
/// Its second thread, where it has one, goes with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Core {
    pub id: u32,
    pub label: String,
    pub chosen: bool,
}

/// How the program spreads over the processor's cores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CpuMode {
    /// The cores a game uses least, as the processor allows.
    #[default]
    Auto,
    All,
    /// The cores ticked.
    Chosen,
}

impl CpuMode {
    pub const ALL: [Self; 3] = [Self::Auto, Self::All, Self::Chosen];

    /// Its place in [`CpuMode::ALL`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&one| one == self).unwrap_or(0)
    }

    /// The mode at `index` of [`CpuMode::ALL`], Auto past its end.
    pub fn at(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

impl Advanced {
    /// The cores show, to tick, while they are the choice.
    pub fn choosing(&self) -> bool {
        CpuMode::at(self.cpu_mode) == CpuMode::Chosen
    }
}

/// The Advanced settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    StartOnSignIn(bool),
    DiscordLinks(bool),
    CloseToTray(bool),
    MultipleWindows(bool),
    DeveloperMode(bool),
    /// A way to spread over the cores, by its place in [`CpuMode::ALL`].
    CpuMode(usize),
    /// A physical core ticked or unticked, by its number.
    Core(u32),
    /// Where streams encode, by the key of the choice.
    Encoder(String),
    /// What draws the interface, by the key of the row ([`DrawOn::key`]).
    Renderer(String),
    /// "Unlocked bitrate" turned on or off; a stream going takes it at
    /// once.
    BitrateUnlocked(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_goes_by_its_place() {
        for mode in CpuMode::ALL {
            assert_eq!(CpuMode::at(mode.index()), mode);
        }
        assert_eq!(CpuMode::at(7), CpuMode::Auto);
        let page = Advanced {
            cpu_mode: CpuMode::Chosen.index(),
            ..Advanced::default()
        };
        assert!(page.choosing());
        assert!(!Advanced::default().choosing());
    }
}
