//! The bitrate limit typed by hand, with Settings' "Unlocked bitrate"
//! (`anomi_vm::voice::share::entry`): a small card the shell draws in the
//! middle of the window, in the share layer, in place of the Go Live
//! dialog or the stream's menu it came from, as the window's dialogs with a
//! field are drawn. "Bitrate limit" over what it takes, the field with
//! "Mbit/s" after it and why a value was refused under it, then "Cancel"
//! and "Apply". Enter in the field sets the value; Escape, once the field
//! has let it go as in every dialog with a field, and a press beside the
//! card leave the limit as it was: the app takes the layer's close for
//! that while the card shows.

use iced::advanced::widget::Id;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Fill};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::voice::share::entry::Refusal;
use anomi_vm::voice::share::{Entry, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;

/// The field, which the app focuses as the card opens, and the button that
/// sets what it holds, for tests.
pub const FIELD: &str = "share-bitrate";
pub const SET: &str = "share-bitrate-set";
/// The card's width, inside it, and between its parts, as the window's
/// small dialogs (`ui/server.slint:526`).
const WIDTH: f32 = 420.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
/// Between the field and its unit.
const UNIT_GAP: f32 = 8.0;
/// The title (`font-size: 18px; font-weight: 700`), as the dialogs'.
const TITLE: f32 = 18.0;

pub fn view(entry: &Entry, motion: Motion) -> Element<'_, Intent> {
    let well = field("20", &entry.text)
        .id(Id::new(FIELD))
        .on_input(Intent::BitrateText)
        .on_submit(Intent::ApplyBitrate)
        .error(entry.refused.map(refusal))
        .motion(motion)
        .view();
    let unit = container(
        text(tr!("Mbit/s"))
            .size(typography::size::BODY)
            .style(catalog::text::secondary),
    )
    .center_y(metrics::CONTROL_HEIGHT);
    let buttons = row![
        action_button(tr!("Cancel"), Kind::Ghost, motion).on_press(Intent::CancelBitrate),
        action_button(tr!("Apply"), Kind::Primary, motion)
            .id(Id::new(SET))
            .on_press(Intent::ApplyBitrate),
    ]
    .spacing(metrics::SPACE_S);
    let card = column![
        text(tr!("Bitrate limit"))
            .size(TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        text(tr!(
            "From 1 to 50 Mbit/s. It's a ceiling: the stream still goes lower when the connection can't carry it."
        ))
        .size(typography::size::SMALL)
        .style(catalog::text::secondary)
        .width(Fill),
        row![well, unit]
            .spacing(UNIT_GAP)
            .align_y(Alignment::Start),
        container(buttons).align_right(Fill),
    ]
    .spacing(SPACING);
    container(card)
        .padding(PADDING)
        .width(WIDTH)
        .style(super::dialog::face)
        .boxed()
}

/// Why the value typed isn't a limit, under the field.
fn refusal(refused: Refusal) -> &'static str {
    match refused {
        Refusal::Empty => tr!("Type how many Mbit/s, such as 20."),
        Refusal::NotANumber => tr!("Type a number, such as 20 or 12.5."),
        Refusal::OutOfRange => tr!("The limit goes from 1 to 50 Mbit/s."),
    }
}
