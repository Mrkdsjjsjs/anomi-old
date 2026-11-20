//! The bitrate limit typed by hand, with Settings' "Unlocked bitrate": a
//! small dialog that "Custom value…" in the limit's steps opens in the
//! share layer, in place of the dialog or the menu it came from. A number
//! of Mbit/s from 1 to 50, with a comma or a point before its decimals, as
//! each language writes them; Enter sets it, Escape leaves the limit as it
//! was.

use super::UNLOCKED_KBPS;

/// The least a limit typed by hand may be, in kbit/s: 1 Mbit/s.
pub const LEAST_KBPS: u32 = 1_000;

/// The hand entry's field: what's typed, and why the last value set was
/// refused, until the next edit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub refused: Option<Refusal>,
}

impl Entry {
    /// The field as it opens: the limit in force, if one is, as the menus
    /// write it with a point; else empty.
    pub fn new(limit: Option<u32>) -> Self {
        Self {
            text: limit.map(written).unwrap_or_default(),
            refused: None,
        }
    }

    /// The person typed: the whole text, and the refusal no longer holds.
    pub fn edit(&mut self, text: String) {
        self.text = text;
        self.refused = None;
    }
}

/// Why a value typed isn't a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Empty,
    /// Not a number: letters, a sign, two decimal marks.
    NotANumber,
    /// Below 1 Mbit/s or above 50.
    OutOfRange,
}

/// The kbit/s that `text` names in Mbit/s, from 1 to 50: digits with at
/// most one comma or point among them, spaces around; at most three
/// decimals count, as a limit is kept in kbit/s.
pub fn parse(text: &str) -> Result<u32, Refusal> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Refusal::Empty);
    }
    let (whole, fraction) = text.split_once([',', '.']).unwrap_or((text, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || !digits(fraction) || (whole.is_empty() && fraction.is_empty()) {
        return Err(Refusal::NotANumber);
    }
    // Leading zeros and digits past the thousandth change nothing a limit
    // can hold; a whole part too long to read is far past the most.
    let whole = whole.trim_start_matches('0');
    if whole.len() > 6 {
        return Err(Refusal::OutOfRange);
    }
    let whole: u32 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| Refusal::NotANumber)?
    };
    let thousandths = (fraction.bytes().chain(std::iter::repeat(b'0')))
        .take(3)
        .fold(0, |sum, digit| sum * 10 + u32::from(digit - b'0'));
    let kbps = whole * 1000 + thousandths;
    if (LEAST_KBPS..=UNLOCKED_KBPS).contains(&kbps) {
        Ok(kbps)
    } else {
        Err(Refusal::OutOfRange)
    }
}

/// `kbps` in Mbit/s as the field shows it: whole, or with a point and the
/// decimals it has ("20", "12.5").
pub fn written(kbps: u32) -> String {
    match kbps % 1000 {
        0 => (kbps / 1000).to_string(),
        rest => {
            let fraction = format!("{rest:03}");
            format!("{}.{}", kbps / 1000, fraction.trim_end_matches('0'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comma_or_a_point_before_the_decimals() {
        assert_eq!(parse("20"), Ok(20_000));
        assert_eq!(parse("20,5"), Ok(20_500));
        assert_eq!(parse("20.5"), Ok(20_500));
        assert_eq!(parse(" 12.25 "), Ok(12_250));
        assert_eq!(parse("1"), Ok(1_000), "the least");
        assert_eq!(parse("50"), Ok(50_000), "the most");
        assert_eq!(parse("50,000"), Ok(50_000));
        assert_eq!(parse("007"), Ok(7_000));
        assert_eq!(parse(",5"), Err(Refusal::OutOfRange), "half a megabit");
        assert_eq!(parse("1.5"), Ok(1_500));
        assert_eq!(parse("9."), Ok(9_000));
        assert_eq!(parse("2.0005"), Ok(2_000), "past the thousandth");
    }

    #[test]
    fn refuses_what_isnt_a_limit() {
        assert_eq!(parse(""), Err(Refusal::Empty));
        assert_eq!(parse("   "), Err(Refusal::Empty));
        for not in ["abc", "1,2,3", "1.2.3", "-5", "+5", "5 Mbit/s", "1e3", ".", "inf", "5 5"] {
            assert_eq!(parse(not), Err(Refusal::NotANumber), "{not}");
        }
        for out in ["0", "0,9", "50.001", "51", "100", "99999999999999999999"] {
            assert_eq!(parse(out), Err(Refusal::OutOfRange), "{out}");
        }
    }

    #[test]
    fn opens_with_the_limit_in_force() {
        assert_eq!(Entry::new(Some(20_000)).text, "20");
        assert_eq!(Entry::new(Some(12_500)).text, "12.5");
        assert_eq!(Entry::new(Some(3_250)).text, "3.25");
        assert_eq!(Entry::new(None).text, "");
        assert_eq!(parse(&written(33_333)), Ok(33_333));
        let mut entry = Entry {
            refused: Some(Refusal::OutOfRange),
            ..Entry::new(None)
        };
        entry.edit("30".into());
        assert_eq!((entry.text.as_str(), entry.refused), ("30", None));
    }
}
