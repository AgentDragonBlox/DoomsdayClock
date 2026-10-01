//! Pure countdown maths: the headline string and its colour. No I/O, no
//! drawing - trivially unit-testable.

use chrono::NaiveDate;

use crate::config::ColorMode;

const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [235, 30, 30];

pub fn days_left(target: NaiveDate, today: NaiveDate) -> i64 {
    (target - today).num_days()
}

/// "47 Days left till 25-12-2026", with singular/today/overdue variants.
pub fn headline(target: NaiveDate, today: NaiveDate) -> String {
    let date = target.format("%d-%m-%Y");
    match days_left(target, today) {
        n if n > 1 => format!("{n} Days left till {date}"),
        1 => format!("1 Day left till {date}"),
        0 => format!("Deadline is today \u{2014} {date}"),
        _ => format!("Deadline passed \u{2014} {date}"),
    }
}

/// Fraction of the countdown already elapsed, 0.0 (just started) to 1.0
/// (deadline reached). Drives both the red fade and the UI progress bar.
pub fn progress(created: NaiveDate, target: NaiveDate, today: NaiveDate) -> f32 {
    let total = (target - created).num_days();
    if total <= 0 {
        return 1.0;
    }
    ((today - created).num_days() as f32 / total as f32).clamp(0.0, 1.0)
}

pub fn text_color(mode: ColorMode, created: NaiveDate, target: NaiveDate, today: NaiveDate) -> [u8; 3] {
    match mode {
        ColorMode::StaticWhite => WHITE,
        ColorMode::FadeToRed => {
            let t = progress(created, target, today);
            std::array::from_fn(|i| (WHITE[i] as f32 + (RED[i] as f32 - WHITE[i] as f32) * t).round() as u8)
        }
    }
}

/// Scales a colour toward black: used for the quote (dimmer than the
/// headline) and author (dimmer still) so they inherit the red fade.
pub fn dim(c: [u8; 3], factor: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * factor).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn headline_variants() {
        assert_eq!(headline(d("2026-12-25"), d("2026-11-07")), "48 Days left till 25-12-2026");
        assert_eq!(headline(d("2026-12-25"), d("2026-12-24")), "1 Day left till 25-12-2026");
        assert!(headline(d("2026-12-25"), d("2026-12-25")).starts_with("Deadline is today"));
        assert!(headline(d("2026-12-25"), d("2026-12-26")).starts_with("Deadline passed"));
    }

    #[test]
    fn fade_endpoints() {
        let (c, t) = (d("2026-10-01"), d("2026-10-11"));
        assert_eq!(text_color(ColorMode::FadeToRed, c, t, c), WHITE);
        assert_eq!(text_color(ColorMode::FadeToRed, c, t, t), RED);
        assert_eq!(text_color(ColorMode::StaticWhite, c, t, t), WHITE);
        let mid = text_color(ColorMode::FadeToRed, c, t, d("2026-10-06"));
        assert!(mid[0] > 240 && mid[1] > 130 && mid[1] < 150);
    }
}
