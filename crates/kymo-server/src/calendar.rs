use kymo_engine::Slot;

use crate::db::Tournament;

/// Seconds since 1970 as `YYYYMMDDTHHMMSSZ`, the form iCalendar wants for UTC times.
pub fn utc_stamp(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    // Civil date from days since the epoch (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Lines longer than 75 bytes are continued on the next line after a space.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut width = 0;
    for c in line.chars() {
        let size = c.len_utf8();
        if width + size > 74 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += size;
    }
    out
}

/// A calendar of every scheduled match, so players can subscribe once and see their games.
pub fn ics(t: &Tournament, stamp: i64) -> String {
    let name = |slot: Slot| match slot {
        Slot::Team(id) => t
            .teams
            .iter()
            .find(|x| i64::from(id) == x.id)
            .map_or("Unknown".into(), |x| x.name.clone()),
        _ => "TBD".into(),
    };
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        "PRODID:-//Kymo//Tournament calendar//EN".to_owned(),
        "CALSCALE:GREGORIAN".to_owned(),
        format!("X-WR-CALNAME:{}", escape(&t.name)),
    ];
    for m in t.bracket.iter().flat_map(|b| &b.matches) {
        let (Some(at), false) = (m.scheduled_at, m.slots.contains(&Slot::Bye)) else {
            continue;
        };
        lines.push("BEGIN:VEVENT".into());
        lines.push(format!("UID:kymo-{}-{}@kymo", t.id, m.id));
        lines.push(format!("DTSTAMP:{}", utc_stamp(stamp)));
        lines.push(format!("DTSTART:{}", utc_stamp(at)));
        lines.push("DURATION:PT45M".into());
        lines.push(format!(
            "SUMMARY:{}",
            escape(&format!("{} vs {}", name(m.slots[0]), name(m.slots[1])))
        ));
        lines.push(format!("DESCRIPTION:{}", escape(&t.name)));
        if let Some(venue) = &m.venue {
            lines.push(format!("LOCATION:{}", escape(venue)));
        }
        lines.push("END:VEVENT".into());
    }
    lines.push("END:VCALENDAR".into());
    lines
        .iter()
        .map(|l| fold(l))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_known_dates() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(1_700_000_000), "20231114T221320Z");
        assert_eq!(utc_stamp(951_782_400), "20000229T000000Z");
    }

    #[test]
    fn escapes_and_folds() {
        assert_eq!(escape("a,b;c\\d\ne"), "a\\,b\\;c\\\\d\\ne");
        let folded = fold(&"x".repeat(160));
        assert!(folded.split("\r\n").all(|l| l.len() <= 75));
        assert_eq!(folded.replace("\r\n ", ""), "x".repeat(160));
    }
}
