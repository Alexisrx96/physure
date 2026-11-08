//! UTC time as RFC 3339 text, for the creation date and the history of a frozen file. The model never reads a
//! clock: the caller passes the time in.

/// The last instant the four-digit format can write: `9999-12-31T23:59:59Z`.
const LAST_WRITABLE: u64 = 253_402_300_799;

/// `secs` since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`; `None` past `9999-12-31T23:59:59Z`.
pub fn rfc3339_from_unix(secs: u64) -> Option<String> {
    if secs > LAST_WRITABLE {
        return None;
    }
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    Some(format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60))
}

/// The civil date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Whether `text` is exactly `YYYY-MM-DDTHH:MM:SSZ` naming a real instant from 1970 on (no leap second).
pub fn is_rfc3339_utc(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return false;
    }
    let num = |from: usize, to: usize| -> Option<u32> {
        let part = &text[from..to];
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok()).flatten()
    };
    let (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) =
        (num(0, 4), num(5, 7), num(8, 10), num(11, 13), num(14, 16), num(17, 19))
    else {
        return false;
    };
    y >= 1970 && (1..=12).contains(&mo) && (1..=days_in_month(y, mo)).contains(&d) && h < 24 && mi < 60 && s < 60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        let at = |secs| rfc3339_from_unix(secs).unwrap();
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(at(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(at(4_102_444_800), "2100-01-01T00:00:00Z");
    }

    #[test]
    fn the_validator() {
        assert!(is_rfc3339_utc("2026-11-07T14:03:11Z"));
        for bad in ["", "2026-11-07", "2026-11-07 14:03:11Z", "2026-11-07T14:03:11", "2026-13-07T14:03:11Z",
                    "2026-11-32T14:03:11Z", "2026-11-07T24:03:11Z", "2026-11-07T14:60:11Z", "2026-11-07T14:03:61Z",
                    "2026-1-07T14:03:11Z", "2026-11-07T14:03:11+00:00"] {
            assert!(!is_rfc3339_utc(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_validator_knows_the_calendar() {
        for bad in ["2026-02-31T00:00:00Z", "2026-04-31T00:00:00Z", "2025-02-29T00:00:00Z", "1900-02-29T00:00:00Z",
                    "0000-01-01T00:00:00Z", "1969-12-31T23:59:59Z", "2026-06-30T23:59:60Z", "2026-00-10T00:00:00Z",
                    "2026-01-00T00:00:00Z"] {
            assert!(!is_rfc3339_utc(bad), "{bad:?}");
        }
        for good in ["2024-02-29T00:00:00Z", "2000-02-29T00:00:00Z", "1970-01-01T00:00:00Z", "9999-12-31T23:59:59Z",
                     "2026-01-31T00:00:00Z", "2026-12-31T23:59:59Z"] {
            assert!(is_rfc3339_utc(good), "{good:?}");
        }
    }

    #[test]
    fn it_writes_four_digit_years_only() {
        assert_eq!(rfc3339_from_unix(253_402_300_799).as_deref(), Some("9999-12-31T23:59:59Z"));
        assert_eq!(rfc3339_from_unix(253_402_300_800), None);
        assert_eq!(rfc3339_from_unix(u64::MAX), None);
    }

    #[test]
    fn what_it_writes_it_accepts() {
        for secs in [0, 86_399, 86_400, 951_782_400, 1_234_567_890, 4_102_444_800, 253_402_300_799] {
            let text = rfc3339_from_unix(secs).unwrap();
            assert!(is_rfc3339_utc(&text), "{text}");
        }
        // every day of a leap year and of a common year round-trips through the validator
        for day in 0..(2 * 366) {
            let secs = 1_704_067_200 + day * 86_400 + 86_399; // from 2024-01-01, last second of each day
            assert!(is_rfc3339_utc(&rfc3339_from_unix(secs).unwrap()));
        }
    }
}
