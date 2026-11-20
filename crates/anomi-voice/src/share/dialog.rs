//! The Go Live dialog's frame, as Discord's (`discord-golive-1-apps.png`):
//! a card of 958 by 596, smaller in a smaller window, 24 px in; across its
//! top the two tabs in a trough, the picked one lit; under them its tab's
//! body; along its bottom the stream's mode over its quality and sound on
//! the left, and on the right the button that starts the stream ("Choose"
//! where the system's dialog picks, "Stream" where a pick here does,
//! "Switch" while live) and the gear, whose menu opens above it.

use iced::advanced::widget::Id;
use iced::widget::{button, column, container, row, svg, text};
use iced::{Alignment, Background, Border, Color, Fill, Length, Shadow, Vector};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::menu::{Align, Spot};
use anomi_vm::voice::share::{Intent, Share, Tab};
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;

/// The card at most (Discord's, measured on `discord-golive-1-apps.png`).
pub const WIDTH: f32 = 958.0;
pub const HEIGHT: f32 = 596.0;
/// The button that starts the stream, for focus and tests.
pub const GO: &str = "share-go";
/// Inside the card, all round.
const PADDING: f32 = 24.0;
/// The tabs' trough, its inside and a tab's corners.
const TABS: f32 = 38.0;
const TROUGH_INSET: f32 = 4.0;
const TAB_RADIUS: f32 = 6.0;
/// A tab's icon, and the room between it and the label.
const TAB_ICON: f32 = 16.0;
const TAB_GAP: f32 = 8.0;
/// The button and the gear along the bottom, and the room between them.
const CONTROL: f32 = 40.0;
const CONTROL_GAP: f32 = 8.0;
/// The gear's corners.
const GEAR_RADIUS: f32 = 8.0;
/// Between the mode's name and the line under it.
const SUMMARY_GAP: f32 = 2.0;
/// The card's shadow, as the menus'.
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
const SHADOW_DROP: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;

pub fn view(share: &Share, motion: Motion) -> Element<'_, Intent> {
    let body = match share.tab {
        Tab::Apps => super::apps::view(share, motion),
        Tab::Devices => super::devices::view(share, motion),
    };
    let card = column![
        tabs(share.tab),
        container(body).width(Fill).height(Fill),
        footer(share, motion),
    ]
    .spacing(PADDING);
    container(card)
        .padding(PADDING)
        .width(Length::Fill.max(WIDTH))
        .height(Length::Fill.max(HEIGHT))
        .style(face)
        .boxed()
}

/// The two tabs, each half the trough.
fn tabs<'a>(shown: Tab) -> Element<'a, Intent> {
    let trough = row![
        tab(Icon::AppWindow, tr!("Applications"), Tab::Apps, shown),
        tab(Icon::Video, tr!("Devices"), Tab::Devices, shown),
    ]
    .spacing(TROUGH_INSET)
    .height(Fill);
    container(trough)
        .padding(TROUGH_INSET)
        .width(Fill)
        .height(TABS)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().sunken)),
            border: Border::default().rounded(metrics::RADIUS_CARD),
            ..container::Style::default()
        })
        .boxed()
}

fn tab<'a>(shape: Icon, label: &'a str, tab: Tab, shown: Tab) -> Element<'a, Intent> {
    let on = tab == shown;
    let face = row![
        icon(shape, TAB_ICON).style(move |theme: &Theme, _| svg::Style {
            color: Some(if on {
                theme.palette().text
            } else {
                theme.palette().muted
            }),
        }),
        text(label)
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(TAB_GAP)
    .align_y(Alignment::Center);
    button(container(face).center(Fill))
        .padding(0)
        .width(Fill)
        .height(Fill)
        .on_press(Intent::Tab(tab))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: (on || lit).then_some(Background::Color(if on {
                    palette.active
                } else {
                    palette.hover
                })),
                text_color: if on || lit {
                    palette.text
                } else {
                    palette.muted
                },
                border: Border::default().rounded(TAB_RADIUS),
                ..button::Style::default()
            }
        })
        .boxed()
}

/// The mode and quality on the left, the button and the gear on the right.
fn footer<'a>(share: &'a Share, motion: Motion) -> Element<'a, Intent> {
    let summary = column![
        text(share.rows.mode.as_str())
            .size(typography::size::TITLE)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None),
        text(share.rows.quality.as_str())
            .size(typography::size::CAPTION)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted),
    ]
    .spacing(SUMMARY_GAP)
    .width(Fill);
    let label = if share.tab == Tab::Apps && share.dialog {
        tr!("Choose What to Share")
    } else if share.switching {
        tr!("Switch")
    } else {
        tr!("Start Streaming")
    };
    let mut go = action_button(label, Kind::Primary, motion)
        .id(Id::new(GO))
        .height(CONTROL)
        .enabled(share.can_go());
    if let Some(intent) = share.go() {
        go = go.on_press(intent);
    }
    let gear = icon_button(Icon::Settings, tr!("Stream settings"), motion)
        .size(CONTROL)
        .radius(GEAR_RADIUS)
        .fill(|palette| palette.button_default)
        .tint(|palette| palette.text)
        .view();
    let gear = opener(gear).on_press(|at| Intent::Gear(Spot::above(at).align(Align::End)));
    row![summary, go, gear]
        .spacing(CONTROL_GAP)
        .align_y(Alignment::Center)
        .boxed()
}

/// The raised card, its edge and its shadow; the hand entry's too.
pub(super) fn face(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        text_color: Some(palette.text),
        ..container::Style::default()
    }
}
