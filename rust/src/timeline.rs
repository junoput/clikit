//! Timelines: events on tracks, drawn along a time axis or one event per row. They go to
//! standard output.

use crate::ctx::{self, Mode, BLUE, CYAN, DIM, GREEN, MAGENTA, RESET, YELLOW};
use crate::layout::fit;
use crate::text::{len, pad, truncate};

/// A timeline with options. [`timeline`] is the short form for the defaults.
///
/// Each row is `[when, track, label]` or `[when, track, label, from]`:
///
/// - `when` is a date (`2026-03-14`, `2026-03-14 09:30`, `2026-03-14T09:30:00+01:00`), a whole
///   number, or any text. When every row has a date, or every row a number, events are sorted
///   and spaced by it; otherwise they keep the order they were given in.
/// - `track` is the line the event sits on: a device, a branch, an album. It may be empty.
/// - `from` names the track this event continues or merges; a line is drawn between the two.
///
/// ```
/// use clikit::Timeline;
///
/// Timeline::new().vertical(true).print(&[
///     vec!["2026-01-10", "main", "first release"],
///     vec!["2026-02-02", "next", "rewrite", "main"],
///     vec!["2026-03-14", "main", "merge the rewrite", "next"],
/// ]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    vertical: bool,
    newest_first: bool,
}

struct Event<'a> {
    when: &'a str,
    track: &'a str,
    label: &'a str,
    from_track: &'a str,
    lane: usize,
    from: Option<usize>,
    at: i64,
}

#[derive(PartialEq)]
enum Kind {
    Date,
    Number,
    Order,
}

impl Timeline {
    /// One line per track along a time axis that is as wide as the terminal.
    pub fn new() -> Timeline {
        Timeline::default()
    }

    /// One row per event and one column per track, oldest first. It is as long as it needs
    /// to be, and it shows the labels, which the view along an axis has no room for.
    pub fn vertical(mut self, on: bool) -> Timeline {
        self.vertical = on;
        self
    }

    /// With [`vertical`](Timeline::vertical): the newest event on top.
    pub fn newest_first(mut self, on: bool) -> Timeline {
        self.newest_first = on;
        self
    }

    /// Print the events. Cells are plain text; they are trimmed. Agents get the events in
    /// order as tab-separated rows, with `from: TRACK` where two tracks are linked.
    pub fn print<R: AsRef<[C]>, C: AsRef<str>>(&self, rows: &[R]) {
        let t = ctx::theme_out();
        let mut events: Vec<Event> = Vec::new();
        let (mut dates, mut numbers, mut midnight) = (0, 0, true);
        let (mut when_width, mut name_width) = (0, 0);
        for row in rows {
            let cell = |i: usize| row.as_ref().get(i).map_or("", |cell| cell.as_ref().trim_matches(' '));
            let (when, track, label, from_track) = (cell(0), cell(1), cell(2), cell(3));
            if when.is_empty() && track.is_empty() && label.is_empty() {
                continue;
            }
            let (kind, at) = parse_when(when);
            match kind {
                Kind::Date => dates += 1,
                Kind::Number => numbers += 1,
                Kind::Order => {}
            }
            midnight &= at % 86400 == 0;
            when_width = when_width.max(len(when));
            name_width = name_width.max(len(track));
            events.push(Event { when, track, label, from_track, lane: 0, from: None, at });
        }
        if events.is_empty() {
            return;
        }
        let n = events.len();
        let kind = if dates == n {
            Kind::Date
        } else if numbers == n {
            Kind::Number
        } else {
            Kind::Order
        };
        // Oldest first; events at the same moment, and events without a date, keep their order.
        let mut order: Vec<usize> = (0..n).collect();
        if kind == Kind::Order {
            for (i, event) in events.iter_mut().enumerate() {
                event.at = i as i64;
            }
        } else {
            order.sort_by_key(|&i| events[i].at);
        }
        // Tracks are numbered in the order they first appear in time.
        let mut names: Vec<&str> = Vec::new();
        for &i in &order {
            let event = &mut events[i];
            event.lane = lane_of(&mut names, event.track);
            if !event.from_track.is_empty() && event.from_track != event.track {
                event.from = Some(lane_of(&mut names, event.from_track));
            }
        }

        let mut text = String::new();
        if t.mode == Mode::Agent {
            for &i in &order {
                let event = &events[i];
                text.push_str(event.when);
                if name_width > 0 {
                    text.push_str(&format!("\t{}", names[event.lane]));
                }
                if let Some(from) = event.from {
                    text.push_str(&format!("\t{}\tfrom: {}", event.label, names[from]));
                } else if !event.label.is_empty() {
                    text.push_str(&format!("\t{}", event.label));
                }
                text.push('\n');
            }
            ctx::out(&text);
            return;
        }

        let palette = [CYAN, MAGENTA, YELLOW, GREEN, BLUE];
        let colors: Vec<&str> = (0..names.len()).map(|l| t.c(palette[l % palette.len()])).collect();
        let (dim, reset) = (t.c(DIM), t.c(RESET));
        let (node, h, v) = (t.g("●", "*"), t.g("─", "-"), t.g("│", "|"));
        let ellipsis = t.g("…", "~");

        if self.vertical {
            if self.newest_first {
                order.reverse();
            }
            // Each track's line runs from the first row that involves it to the last.
            let mut span: Vec<Option<(usize, usize)>> = vec![None; names.len()];
            for (r, &i) in order.iter().enumerate() {
                for lane in [Some(events[i].lane), events[i].from].into_iter().flatten() {
                    span[lane] = Some((span[lane].map_or(r, |(first, _)| first), r));
                }
            }
            let active = |lane: usize, r: usize| span[lane].is_some_and(|(first, last)| first <= r && r <= last);
            let room = if t.mode == Mode::Rich {
                // A label too long for the terminal is cut.
                let mut room = ctx::width() as isize - 2 * names.len() as isize - 3;
                if when_width > 0 {
                    room -= when_width as isize + 2;
                }
                if name_width > 0 {
                    room -= name_width as isize + 2;
                }
                room.max(4) as usize
            } else {
                0
            };
            for (r, &i) in order.iter().enumerate() {
                let event = &events[i];
                let (a, b) = (event.lane, event.from);
                let (left, right) = match b {
                    Some(b) => (a.min(b), a.max(b)),
                    None => (a, a),
                };
                let mut line = String::new();
                for lane in 0..names.len() {
                    let glyph = if lane == a {
                        node
                    } else if Some(lane) == b {
                        let (first, last) = span[lane].unwrap_or((r, r));
                        match (lane > a, first < r, last > r) {
                            (true, true, true) => t.g("┤", "+"),
                            (true, true, false) => t.g("╯", "+"),
                            (true, false, true) => t.g("╮", "+"),
                            (false, true, true) => t.g("├", "+"),
                            (false, true, false) => t.g("╰", "+"),
                            (false, false, true) => t.g("╭", "+"),
                            (_, false, false) => h,
                        }
                    } else if lane > left && lane < right {
                        if active(lane, r) {
                            t.g("┼", "+")
                        } else {
                            h
                        }
                    } else if active(lane, r) {
                        v
                    } else {
                        " "
                    };
                    if glyph == " " {
                        line.push(' ');
                    } else {
                        line.push_str(&format!("{}{glyph}{reset}", colors[lane]));
                    }
                    if lane + 1 < names.len() {
                        match b {
                            Some(b) if lane >= left && lane < right => {
                                line.push_str(&format!("{}{h}{reset}", colors[b]));
                            }
                            _ => line.push(' '),
                        }
                    }
                }
                let label = if room > 0 && len(event.label) > room {
                    format!("{}{ellipsis}", truncate(event.label, room - 1))
                } else {
                    event.label.to_string()
                };
                let mut tail = label;
                if name_width > 0 {
                    let (track, color) = (names[a], colors[a]);
                    if !tail.is_empty() {
                        tail = format!("{color}{}{reset}  {tail}", pad(track, name_width));
                    } else if !track.is_empty() {
                        tail = format!("{color}{track}{reset}");
                    }
                }
                if when_width > 0 {
                    if !tail.is_empty() {
                        tail = format!("{dim}{}{reset}  {tail}", pad(event.when, when_width));
                    } else if !event.when.is_empty() {
                        tail = format!("{dim}{}{reset}", event.when);
                    }
                }
                if tail.is_empty() {
                    text.push_str(&format!("  {}\n", line.trim_end_matches(' ')));
                } else {
                    text.push_str(&format!("  {line}  {tail}\n"));
                }
            }
            ctx::out(&text);
            return;
        }

        // Across: a time axis, then one line per track.
        let width = ctx::width();
        let name_width = names.iter().map(|name| len(name)).max().unwrap_or(0).min((width / 3).max(8));
        let mut plot = width as i64 - 2;
        if name_width > 0 {
            plot -= name_width as i64 + 2;
        }
        let plot = plot.max(10);
        let (min, max) = (events[order[0]].at, events[order[n - 1]].at);
        let column = |at: i64| -> i64 {
            if max == min {
                0
            } else {
                ((at - min) * (plot - 1) * 2 + max - min) / (2 * (max - min))
            }
        };
        // Each track's line runs from the first column that involves it to the last.
        let columns: Vec<usize> = events.iter().map(|event| column(event.at) as usize).collect();
        let mut span: Vec<Option<(usize, usize)>> = vec![None; names.len()];
        for &i in &order {
            for lane in [Some(events[i].lane), events[i].from].into_iter().flatten() {
                span[lane] = Some((span[lane].map_or(columns[i], |(first, _)| first), columns[i]));
            }
        }
        let mut grid: Vec<Vec<char>> = span
            .iter()
            .map(|span| {
                let (first, last) = span.unwrap_or((0, 0));
                let mut cells = vec![' '; first];
                cells.extend(std::iter::repeat('-').take(last - first + 1));
                cells
            })
            .collect();
        let put = |cells: &mut Vec<char>, column: usize, code: char| {
            if cells.len() <= column {
                cells.resize(column, ' ');
                cells.push(code);
            } else {
                cells[column] = code;
            }
        };
        let is_event = |cells: &[char], column: usize| matches!(cells.get(column), Some('o' | 'O'));
        // The events; O marks a column that holds more than one.
        for (i, event) in events.iter().enumerate() {
            let code = if is_event(&grid[event.lane], columns[i]) { 'O' } else { 'o' };
            put(&mut grid[event.lane], columns[i], code);
        }
        // The links between tracks.
        for &i in &order {
            let (a, column) = (events[i].lane, columns[i]);
            let Some(b) = events[i].from else { continue };
            if !is_event(&grid[b], column) {
                let (first, last) = span[b].unwrap_or((column, column));
                let code = match (b < a, first < column, last > column) {
                    (true, true, true) => 'a',
                    (true, true, false) => 'b',
                    (true, false, true) => 'c',
                    (false, true, true) => 'e',
                    (false, true, false) => 'f',
                    (false, false, true) => 'g',
                    (_, false, false) => 'd',
                };
                put(&mut grid[b], column, code);
            }
            for lane in a.min(b) + 1..a.max(b) {
                if !is_event(&grid[lane], column) {
                    let crossing = span[lane].is_some_and(|(first, last)| first <= column && column <= last);
                    put(&mut grid[lane], column, if crossing { 'x' } else { 'd' });
                }
            }
        }

        // The axis: a label, unless it would run into the one before or off the end.
        let mut axis = String::new();
        let mut next = 0;
        let mut tick = |column: i64, label: &str| {
            if column < next || column + len(label) as i64 > plot {
                return;
            }
            axis = format!("{}{label}", pad(&axis, column as usize));
            next = column + len(label) as i64 + 2;
        };
        // The first step whose labels have room, else the last.
        let step = |unit: i64, label_width: i64, steps: &[i64]| -> i64 {
            let need = (label_width + 2) * (max - min);
            *steps.iter().find(|step| **step * (plot - 1) * unit >= need).unwrap_or(&steps[steps.len() - 1])
        };
        match kind {
            Kind::Order => {
                tick(0, events[order[0]].when);
                if n > 1 {
                    let last = events[order[n - 1]].when;
                    tick(plot - len(last) as i64, last);
                }
            }
            Kind::Number => {
                tick(0, &min.to_string());
                if max != min {
                    let last = max.to_string();
                    tick(plot - len(&last) as i64, &last);
                }
            }
            Kind::Date => {
                let mut day = min.div_euclid(86400);
                let (mut y, mut m, d) = civil(day);
                if max - min >= 63_072_000 {
                    // Two years or more: years.
                    let every = step(31_536_000, 4, &[1, 2, 5, 10, 20, 50, 100, 200, 500, 1000]);
                    tick(0, &format!("{y:04}"));
                    y = y / every * every;
                    loop {
                        y += every;
                        let at = days(y, 1, 1) * 86400;
                        if at > max {
                            break;
                        }
                        tick(column(at), &format!("{y:04}"));
                    }
                } else if max - min >= 5_184_000 {
                    // Sixty days or more: months.
                    let every = step(2_419_200, 7, &[1, 2, 3, 6]);
                    tick(0, &format!("{y:04}-{m:02}"));
                    loop {
                        m += 1;
                        if m > 12 {
                            (y, m) = (y + 1, 1);
                        }
                        let at = days(y, m, 1) * 86400;
                        if at > max {
                            break;
                        }
                        if (m - 1) % every == 0 {
                            tick(column(at), &format!("{y:04}-{m:02}"));
                        }
                    }
                } else if max - min >= 172_800 || midnight {
                    // Days.
                    let every = step(86400, 5, &[1, 2, 7, 14]);
                    tick(0, &format!("{y:04}-{m:02}-{d:02}"));
                    loop {
                        day += 1;
                        let at = day * 86400;
                        if at > max {
                            break;
                        }
                        if day.rem_euclid(every) == 0 {
                            let (_, m, d) = civil(day);
                            tick(column(at), &format!("{m:02}-{d:02}"));
                        }
                    }
                } else {
                    // Under two days: hours.
                    let every = step(3600, 5, &[1, 2, 3, 6, 12]);
                    let second = min - day * 86400;
                    tick(0, &format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", second / 3600, second % 3600 / 60));
                    let mut hour = day * 24 + second / 3600;
                    loop {
                        hour += 1;
                        let at = hour * 3600;
                        if at > max {
                            break;
                        }
                        let of_day = hour.rem_euclid(24);
                        if of_day % every == 0 {
                            tick(column(at), &format!("{of_day:02}:00"));
                        }
                    }
                }
            }
        }
        if !axis.is_empty() {
            let indent = if name_width > 0 { name_width + 2 } else { 0 };
            text.push_str(&format!("  {}{dim}{axis}{reset}\n", " ".repeat(indent)));
        }
        for (lane, cells) in grid.iter().enumerate() {
            let mut line = String::new();
            if name_width > 0 {
                line.push_str(&format!("{}  ", fit(names[lane], name_width, 'l', ellipsis)));
            }
            for code in cells {
                line.push_str(match code {
                    '-' => h,
                    'o' => node,
                    'O' => t.g("◉", "#"),
                    'x' => t.g("┼", "+"),
                    'a' => t.g("┬", "+"),
                    'b' => t.g("╮", "+"),
                    'c' => t.g("╭", "+"),
                    'd' => v,
                    'e' => t.g("┴", "+"),
                    'f' => t.g("╯", "+"),
                    'g' => t.g("╰", "+"),
                    _ => " ",
                });
            }
            text.push_str(&format!("  {}{line}{reset}\n", colors[lane]));
        }
        ctx::out(&text);
    }
}

/// A timeline along a time axis: one line per track, as wide as the terminal. See [`Timeline`]
/// for the rows and for the view with one event per row.
///
/// ```
/// clikit::timeline(&[
///     ["2024-02-01", "iPhone 12", "38.7 GB"],
///     ["2025-06-01", "iPhone 12", "39.4 GB"],
///     ["2026-11-20", "iPad", "12.8 GB"],
/// ]);
/// ```
pub fn timeline<R: AsRef<[C]>, C: AsRef<str>>(rows: &[R]) {
    Timeline::new().print(rows);
}

/// The number of the track with that name; a new name is a new track.
fn lane_of<'a>(names: &mut Vec<&'a str>, name: &'a str) -> usize {
    names.iter().position(|known| *known == name).unwrap_or_else(|| {
        names.push(name);
        names.len() - 1
    })
}

/// A date as seconds since 1970, a whole number as itself, or neither.
///
/// A date is `YYYY-MM-DD`, then optionally `T` or a space and `HH:MM` or `HH:MM:SS`, then
/// optionally a time zone (`Z`, `+01:00`, ` +0100`), which is read and ignored.
fn parse_when(text: &str) -> (Kind, i64) {
    let b = text.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let two = |i: usize| i64::from(b[i] - b'0') * 10 + i64::from(b[i + 1] - b'0');
    let is = |i: usize, c: u8| b.get(i) == Some(&c);
    if (0..10).all(|i| if i == 4 || i == 7 { is(i, b'-') } else { digit(i) }) {
        let mut end = 10;
        let mut seconds = 0;
        if (is(10, b'T') || is(10, b' ')) && digit(11) && digit(12) && is(13, b':') && digit(14) && digit(15) {
            seconds = two(11) * 3600 + two(14) * 60;
            end = 16;
            if is(16, b':') && digit(17) && digit(18) {
                seconds += two(17);
                end = 19;
            }
        }
        let zone = if is(end, b' ') { end + 1 } else { end };
        let zoned = if is(zone, b'Z') {
            b.len() == zone + 1
        } else if (is(zone, b'+') || is(zone, b'-')) && digit(zone + 1) && digit(zone + 2) {
            let minutes = if is(zone + 3, b':') { zone + 4 } else { zone + 3 };
            digit(minutes) && digit(minutes + 1) && b.len() == minutes + 2
        } else {
            false
        };
        if b.len() == end || zoned {
            let year = two(0) * 100 + two(2);
            return (Kind::Date, days(year, two(5), two(8)) * 86400 + seconds);
        }
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    if (1..=15).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(number) = text.parse::<i64>() {
            return (Kind::Number, number);
        }
    }
    (Kind::Order, 0)
}

/// Days since 1970-01-01 for a date in the Gregorian calendar.
fn days(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let day_of_year = (153 * if month > 2 { month - 3 } else { month + 9 } + 2) / 5 + day - 1;
    era * 146_097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year - 719_468
}

/// The year, month and day that is `days` after 1970-01-01.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_convert_both_ways() {
        assert_eq!(days(1970, 1, 1), 0);
        assert_eq!(days(2000, 3, 1), 11_017);
        assert_eq!(days(1969, 12, 31), -1);
        for day in [-800_000, -1, 0, 59, 11_016, 11_017, 20_783, 2_932_896] {
            let (y, m, d) = civil(day);
            assert_eq!(days(y, m, d), day, "{y}-{m}-{d}");
        }
        assert_eq!(civil(20_783), (2026, 11, 26));
    }

    #[test]
    fn when_is_a_date_a_number_or_neither() {
        let at = |text: &str| parse_when(text).1;
        let kind = |text: &str| parse_when(text).0;
        assert!(kind("2026-11-26") == Kind::Date);
        assert_eq!(at("2026-11-26"), 20_783 * 86400);
        assert_eq!(at("2026-11-26 09:30"), 20_783 * 86400 + 34_200);
        assert_eq!(at("2026-11-26T09:30:15+01:00"), 20_783 * 86400 + 34_215);
        assert_eq!(at("2026-11-26 09:30:15 +0100"), 20_783 * 86400 + 34_215);
        assert_eq!(at("2026-11-26+01:00"), 20_783 * 86400);
        assert_eq!(at("2026-11-26T09:30Z"), 20_783 * 86400 + 34_200);
        assert!(kind("2026-11-26 9:30") == Kind::Order);
        assert!(kind("2026-11-26T09:30:1") == Kind::Order);
        assert!(kind("2026-11-26 noon") == Kind::Order);
        assert!(kind("2026-11") == Kind::Order);
        assert!(kind("-40") == Kind::Number);
        assert_eq!(at("-40"), -40);
        assert_eq!(at("0007"), 7);
        assert!(kind("1234567890123456") == Kind::Order);
        assert!(kind("v1.0") == Kind::Order);
        assert!(kind("") == Kind::Order);
    }
}
