//! The sign-in screen and linking Discord: what it shows and what it asks
//! for. The Slint side: `SignInScreen` (`ui/sign-in.slint:8`) and
//! `DiscordLinkPanel` (`ui/discord.slint:29`), filled by `show_workspace`
//! (`src/ui/refresh.rs:960`) and answered by `src/ui/callbacks.rs:1308`.
//! Task N09 (docs/iced/tasks/shell.md).
//!
//! The token's text is held here, as the Slint window's in-out properties
//! were: the app keeps what was typed and projects it back. Its `Debug`
//! leaves the token out, so no log line holds it.

use std::fmt;
use std::rc::Rc;

use iced::widget::qr_code;

/// The sign-in screen and linking Discord: what its screen draws. The window
/// holds it as `vm.sign_in`.
#[derive(Clone, PartialEq)]
pub struct SignIn {
    /// The app's name beside its icon at the top left (`BuildInfo.product`).
    pub product: String,
    /// A sign-in or a token check is under way: fields and buttons are off
    /// (`busy`).
    pub busy: bool,
    /// What the last attempt said: red, or grey while `busy` (`notice`).
    pub notice: String,
    /// Where linking Discord has got to.
    pub link: Link,
    /// The token field is open under the link's buttons (`token-open`).
    pub token_open: bool,
    /// What was pasted in it (`token`).
    pub token: String,
    /// The graphics' notice over the card, with Dismiss (`graphics-notice`).
    pub graphics_notice: Option<String>,
}

impl Default for SignIn {
    fn default() -> Self {
        Self {
            product: "Anomi".into(),
            busy: false,
            notice: String::new(),
            link: Link::default(),
            token_open: false,
            token: String::new(),
            graphics_notice: None,
        }
    }
}

/// Where linking Discord has got to (`phase`, `ui/discord.slint:24`: 0 to 6).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Link {
    /// Not linking: the warning and the buttons (0).
    #[default]
    Idle,
    /// Getting a code (1).
    Starting,
    /// The code to scan, or none when it could not be drawn (2).
    Code(Option<Code>),
    /// The phone scanned it, as this name (3).
    Scanned(String),
    /// Linking failed, saying why (4).
    Failed(String),
    /// Discord's captcha waits in the browser (5).
    Captcha,
    /// Discord checks a pasted token (6).
    Checking,
}

/// The QR code Discord's phone app scans, made once from the address the
/// core hands over (`qr_code`, `src/ui/pictures.rs:13`) and shared by every
/// copy of the view-model.
#[derive(Clone, Debug)]
pub struct Code {
    address: String,
    data: Rc<qr_code::Data>,
    modules: usize,
}

impl Code {
    /// The code carrying `address`, or `None` when it does not fit one.
    pub fn new(address: &str) -> Option<Self> {
        Some(Self {
            address: address.to_owned(),
            data: Rc::new(qr_code::Data::new(address.as_bytes()).ok()?),
            // iced's `Data` keeps its width private; the same encoder says it.
            modules: qrcode::QrCode::new(address.as_bytes()).ok()?.width(),
        })
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// Modules along one side of the code, without any quiet zone.
    pub fn modules(&self) -> usize {
        self.modules
    }

    pub fn data(&self) -> &qr_code::Data {
        &self.data
    }
}

impl PartialEq for Code {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

/// The sign-in screen and linking Discord: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Open the local preview (`explore-preview`).
    ExplorePreview,
    /// Show the code to scan (`link-discord`).
    LinkDiscord,
    /// Open the token field; the app focuses `sign_in::TOKEN`.
    OpenToken,
    /// The token field was edited.
    Token(String),
    /// Link with the token typed in (`link-discord-token`). The app takes it
    /// from [`SignIn::token`], so no copy of the secret rides in the
    /// messages each view makes.
    LinkToken,
    /// Open the captcha's page in the browser again (`open-discord-captcha`).
    OpenCaptcha,
    /// Put the graphics' notice away.
    DismissNotice,
}

/// A secret as a log line shows it: whether one was typed, never what.
pub(crate) struct Hidden<'a>(pub &'a str);

impl fmt::Debug for Hidden<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_empty() {
            "\"\""
        } else {
            "<hidden>"
        })
    }
}

impl fmt::Debug for SignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Every field named, so a new one is a choice to show or hide.
        let Self {
            product,
            busy,
            notice,
            link,
            token_open,
            token,
            graphics_notice,
        } = self;
        f.debug_struct("SignIn")
            .field("product", product)
            .field("busy", busy)
            .field("notice", notice)
            .field("link", link)
            .field("token_open", token_open)
            .field("token", &Hidden(token))
            .field("graphics_notice", graphics_notice)
            .finish()
    }
}

impl fmt::Debug for Intent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Token(secret) => f.debug_tuple("Token").field(&Hidden(secret)).finish(),
            Self::ExplorePreview => f.write_str("ExplorePreview"),
            Self::LinkDiscord => f.write_str("LinkDiscord"),
            Self::LinkToken => f.write_str("LinkToken"),
            Self::OpenToken => f.write_str("OpenToken"),
            Self::OpenCaptcha => f.write_str("OpenCaptcha"),
            Self::DismissNotice => f.write_str("DismissNotice"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Intent, SignIn};
    use crate::settings::mfa::{self, Mfa};

    const SECRET: &str = "mfa.hunter2-secret";

    #[test]
    fn no_secret_reaches_a_log_line() {
        let draft = SignIn {
            token: SECRET.into(),
            notice: "sam".into(),
            ..SignIn::default()
        };
        let prompt = Mfa {
            code: SECRET.into(),
            ..Mfa::default()
        };
        let lines = [
            format!("{draft:?}"),
            format!("{prompt:?}"),
            format!("{:?}", Intent::Token(SECRET.into())),
            format!("{:?}", mfa::Intent::Write(SECRET.into())),
            format!("{:?}", crate::Intent::SignIn(Intent::Token(SECRET.into()))),
        ];
        for line in &lines {
            assert!(!line.contains("hunter2"), "{line}");
            assert!(line.contains("<hidden>"), "{line}");
        }
        assert!(lines[0].contains("\"sam\""), "the rest shows");
    }
}
