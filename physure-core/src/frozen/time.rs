//! UTC time as RFC 3339 text, for the creation date and the history of a frozen file. The model never reads a
//! clock: the caller passes the time in.

/// `secs` since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn rfc3339_from_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
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

/// Whether `text` is exactly `YYYY-MM-DDTHH:MM:SSZ` with a plausible date and time.
pub fn is_rfc3339_utc(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return false;
    }
    let num = |from: usize, to: usize| -> Option<u32> {
        let part = &text[from..to];
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok()).flatten()
    };
    let (Some(_y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) =
        (num(0, 4), num(5, 7), num(8, 10), num(11, 13), num(14, 16), num(17, 19))
    else {
        return false;
    };
    (1..=12).contains(&mo) && (1..=31).contains(&d) && h < 24 && mi < 60 && s < 61
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_from_unix(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(rfc3339_from_unix(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(rfc3339_from_unix(4_102_444_800), "2100-01-01T00:00:00Z");
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
    fn what_it_writes_it_accepts() {
        for secs in [0, 86_399, 86_400, 1_234_567_890, 4_102_444_800] {
            assert!(is_rfc3339_utc(&rfc3339_from_unix(secs)));
        }
    }
}
