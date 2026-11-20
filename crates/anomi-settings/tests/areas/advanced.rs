//! The Advanced settings page: task S10's tests, driving
//! `anomi_settings::advanced::view` over its fixtures in the simulator
//! (`super::support`), and Optimization's choices. "Interface rendering"
//! comes before "Encode streams on" and names the same adapters and the
//! processor, so a click by name finds its row first.

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::advanced::view;
use anomi_vm::fixtures::settings::advanced as sample;
use anomi_vm::settings::advanced::{Advanced, CpuMode, Intent};

use super::support::{SIZE, asked, front, settings};

/// With Optimization the page runs past the window's height, where the
/// frame scrolls it: the simulator gets room for all of it.
fn simulate(page: &Advanced) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    simulate_at(page, SIZE.width)
}

/// [`simulate`] `width` wide.
fn simulate_at(
    page: &Advanced,
    width: f32,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    let tall = iced::Size::new(width, 1800.0);
    Simulator::with_size(settings(), tall, view(page, front()).boxed())
}

#[test]
fn a_switch_asks_for_its_opposite() {
    let page = sample::advanced();
    let mut ui = simulate(&page);
    for name in [
        "Start when you sign in",
        "Open Discord links in Yapfast",
        "Close to tray",
        "Allow multiple windows",
        "Developer Mode",
    ] {
        ui.click(name).expect("the switch is there");
    }
    assert_eq!(
        asked(ui),
        [
            Intent::StartOnSignIn(true),
            Intent::DiscordLinks(true),
            Intent::CloseToTray(false),
            Intent::MultipleWindows(true),
            Intent::DeveloperMode(true),
        ]
    );
}

#[test]
fn the_gpu_rendering_switch_is_a_choice_here_with_no_restart() {
    let page = sample::advanced();
    let mut ui = simulate(&page);
    assert!(ui.find("Render with the GPU").is_err());
    assert!(ui.find("Restart Yapfast").is_err());
    assert!(ui.find("Interface rendering").is_ok());
    assert!(ui.find("Automatic").is_ok());
    assert!(
        ui.find(page.renderer_line.as_str()).is_ok(),
        "what draws now"
    );
}

#[test]
fn interface_rendering_chooses_by_key_and_not_an_adapter_that_cant_draw() {
    let page = sample::advanced();
    // The page's column in the small window, 1000 wide less the sections
    // (266) and the column's sides (32 each): every row is there to press.
    let mut ui = simulate_at(&page, 670.0);
    let radeon = &page.renderers[3];
    assert!(ui.find(radeon.description.as_str()).is_ok(), "says why");
    ui.click("Processor").expect("the processor");
    ui.click("NVIDIA GeForce RTX 3080")
        .expect("each adapter by name");
    ui.click("AMD Radeon 610M").expect("shown, faded");
    ui.click("Automatic").expect("and Automatic");
    assert_eq!(
        asked(ui),
        [
            Intent::Renderer("processor".into()),
            Intent::Renderer("pci:0000:01:00.0".into()),
            Intent::Renderer("auto".into()),
        ],
        "the Radeon, without its Vulkan driver, answers no click"
    );
}

#[test]
fn optimization_chooses_how_the_cores_are_spread_and_says_so() {
    let page = sample::advanced();
    let mut ui = simulate(&page);
    assert!(ui.find(page.cpu_line.as_str()).is_ok(), "the line under it");
    assert!(ui.find("15").is_err(), "the cores show only to choose");
    ui.click("Choose cores").expect("the choice is there");
    assert_eq!(asked(ui), [Intent::CpuMode(CpuMode::Chosen.index())]);
    let choosing = Advanced {
        cpu_mode: CpuMode::Chosen.index(),
        ..sample::advanced()
    };
    let mut ui = simulate(&choosing);
    assert!(ui.find("Chiplet 1 · L3 96 MB").is_ok());
    ui.click("3").expect("a core of the first chiplet");
    ui.click("12").expect("a core of the second");
    assert_eq!(asked(ui), [Intent::Core(3), Intent::Core(12)]);
}

#[test]
fn optimization_chooses_where_streams_encode_by_key() {
    // Without the renderer's rows, whose names come first.
    let page = Advanced {
        renderers: Vec::new(),
        ..sample::advanced()
    };
    let mut ui = simulate(&page);
    assert!(ui.find(page.encoder_line.as_str()).is_ok());
    ui.click("NVIDIA GeForce RTX 3080")
        .expect("each card by name");
    ui.click("Processor").expect("and the processor");
    assert_eq!(
        asked(ui),
        [
            Intent::Encoder("pci:0000:01:00.0".into()),
            Intent::Encoder("software".into()),
        ]
    );
}

/// "Unlocked bitrate" is a switch under Optimization, off unless turned on,
/// saying in plain words what passing Discord's 9 Mbit/s risks.
#[test]
fn the_bitrate_unlock_is_a_switch_that_says_what_it_risks() {
    let page = sample::advanced();
    assert!(!page.bitrate_unlocked, "off unless turned on");
    let mut ui = simulate(&page);
    assert!(ui.find("Unlocked bitrate").is_ok());
    let risks = ui
        .find(
            "Lets a stream's bitrate limit be set by hand above 9 Mbit/s, up to 50. Discord documents 9 Mbit/s as the most; what its servers do with more isn't known: the stream may be cut off, lose packets or be dropped. A higher bitrate also needs an upload that fast, and loads the encoder and the people watching.",
        )
        .expect("what it risks, under it")
        .bounds();
    let switch = ui.find("Unlocked bitrate").unwrap().bounds();
    assert!(risks.y > switch.y);
    ui.click("Unlocked bitrate").expect("the switch is there");
    assert_eq!(asked(ui), [Intent::BitrateUnlocked(true)]);
    let on = Advanced {
        bitrate_unlocked: true,
        ..sample::advanced()
    };
    let mut ui = simulate(&on);
    ui.click("Unlocked bitrate").expect("the switch is there");
    assert_eq!(asked(ui), [Intent::BitrateUnlocked(false)]);
}

#[test]
fn switches_the_system_lacks_are_left_out() {
    let page = Advanced {
        autostart_available: false,
        discord_links_available: false,
        ..sample::advanced()
    };
    let mut ui = simulate(&page);
    assert!(ui.find("Start when you sign in").is_err());
    assert!(ui.find("Open Discord links in Yapfast").is_err());
}
