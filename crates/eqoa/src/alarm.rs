//! The game's two clock commands, the quick chat's Time and Alarm (and
//! `/time`, `/alarm`): `VITimeOfDay::GetLocalTime` (SUPPORT
//! vitimeofday.cpp), the console's clock as "Wed Oct 21 00:30:12 2026", and
//! `VIAlarm` (CLIENT vialarm.cpp), a reminder after so much play.

/// `GetDayOfWeek(month, day, year, dow)`: 0 Monday .. 6 Sunday, as shipped
/// (the leap year's day comes off January only, so a leap February is a
/// day out).
pub fn day_of_week(month: i32, day: i32, year: i32) -> i32 {
    let leap = if year & 3 == 0 { if year % 100 != 0 { true } else { year % 400 == 0 } } else { false };
    let c = year % 100;
    let mut dow = c + c / 4;
    dow += match month {
        1 => 6,
        2 | 3 => 2,
        4 => 5,
        6 => 3,
        7 => 5,
        8 => 1,
        9 => 4,
        10 => 6,
        11 => 2,
        12 => 4,
        _ => 0,
    };
    dow += day;
    if year < 2000 {
        dow += 1;
    }
    if leap && (month as u32) < 2 {
        dow -= 1;
    }
    dow %= 7;
    if dow <= 0 {
        dow += 7;
    }
    dow - 1
}

/// `GetLocalTime`: "%s %s %d %02d:%02d:%02d 20%02d" from the clock's
/// (year, month, day, hour, minute, second).
pub fn local_time_text(t: (i32, i32, i32, i32, i32, i32)) -> String {
    let (year, month, day, hour, minute, second) = t;
    let yy = year.rem_euclid(100);
    let dow = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].get(day_of_week(month, day, yy + 2000) as usize).copied().unwrap_or("???");
    let mon = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].get((month - 1) as usize).copied().unwrap_or("???");
    format!("{dow} {mon} {day} {hour:02}:{minute:02}:{second:02} 20{yy:02}")
}

/// The machine's local time, as the console's RTC gives its own
/// (`sceCdReadClock` + `sceScfGetLocalTimefromRTC`): (year, month 1..12,
/// day, hour, minute, second).
pub fn local_time() -> (i32, i32, i32, i32, i32, i32) {
    #[cfg(target_arch = "wasm32")]
    {
        let d = js_sys::Date::new_0();
        (d.get_full_year() as i32, d.get_month() as i32 + 1, d.get_date() as i32, d.get_hours() as i32, d.get_minutes() as i32, d.get_seconds() as i32)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        native_local(secs).unwrap_or_else(|| civil(secs))
    }
}

/// `localtime_r`'s first nine fields (`struct tm` begins with them on every
/// libc).
#[cfg(all(unix, not(target_arch = "wasm32")))]
fn native_local(secs: i64) -> Option<(i32, i32, i32, i32, i32, i32)> {
    unsafe extern "C" {
        fn localtime_r(t: *const i64, tm: *mut [i64; 16]) -> *mut [i64; 16];
    }
    let mut tm = [0i64; 16];
    // SAFETY: tm is larger than any libc's `struct tm`; time_t is 64-bit here
    let ok = unsafe { !localtime_r(&secs, &mut tm).is_null() };
    let f: [i32; 9] = std::array::from_fn(|k| unsafe { *(tm.as_ptr() as *const i32).add(k) });
    ok.then_some((f[5] + 1900, f[4] + 1, f[3], f[2], f[1], f[0]))
}

#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
fn native_local(_secs: i64) -> Option<(i32, i32, i32, i32, i32, i32)> {
    None
}

/// UTC from the Unix time, where the zone is not known.
#[allow(dead_code)]
fn civil(secs: i64) -> (i32, i32, i32, i32, i32, i32) {
    let (days, s) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    (y as i32, m as i32, d as i32, (s / 3600) as i32, (s / 60 % 60) as i32, (s % 60) as i32)
}

/// `snprintf16`'s `%d` and `%02d` (and `%%`) for a word's numbers.
pub fn format_ints(fmt: &str, args: &[i32]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut c = fmt.chars().peekable();
    while let Some(ch) = c.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        let mut spec = String::new();
        while let Some(&d) = c.peek()
            && d.is_ascii_digit()
        {
            spec.push(d);
            c.next();
        }
        match c.next() {
            Some('d') | Some('i') | Some('u') => {
                let v = args.next().copied().unwrap_or(0);
                let width: usize = spec.trim_start_matches('0').parse().unwrap_or(0);
                if spec.starts_with('0') {
                    out.push_str(&format!("{v:0width$}"));
                } else {
                    out.push_str(&format!("{v:width$}"));
                }
            }
            Some('%') => out.push('%'),
            Some(o) => {
                out.push('%');
                out.push_str(&spec);
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    out
}

/// What the alarm asks of the game: a line for the chat box (a word, or a
/// word's text), or the message box it rings with.
#[derive(Clone, Debug, PartialEq)]
pub enum AlarmOut {
    Word(i32),
    Text(String),
    Prompt(String),
}

/// `VIAlarm` (`m_alarm`): on for so many hours and minutes of play.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Alarm {
    pub on: bool,
    pub hours: i32,
    pub minutes: i32,
    /// `m_when`: the client's clock (ms) it rings at
    pub when: i64,
}

impl Alarm {
    /// `Command(arg)`: "Off" (word 552, any case) turns it off; otherwise
    /// only digits and ':' (so "H.MM", which the parse would take, is
    /// refused: word 1402), at most five characters: "H:MM" or minutes.
    pub fn command(&mut self, arg: &str, off: &str, now: i64, words: &dyn Fn(i32) -> String) -> Vec<AlarmOut> {
        if arg.eq_ignore_ascii_case(off) {
            return self.set(false, 0, 0, now, words);
        }
        if arg.chars().any(|c| (c as u32).wrapping_sub('0' as u32) as u16 > 10) {
            return vec![AlarmOut::Word(1402)];
        }
        let buf: String = arg.chars().take(5).collect();
        let sep = buf.find([':', '.']);
        let num = |s: &str| -> Option<i32> {
            let t = s.trim_start();
            let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() { None } else { Some(digits.parse::<i64>().map_or(i32::MAX, |v| v as i32)) }
        };
        let (hours, minutes) = match sep {
            Some(i) => (num(&buf).unwrap_or(0), num(&buf[i + 1..]).unwrap_or(0)),
            None => (0, num(&buf).unwrap_or(0)),
        };
        self.set(true, hours, minutes, now, words)
    }

    /// `Set(on, hours, minutes)`: a time of nothing refused (word 1402);
    /// turned off, word 1202; on, word 1200 with the time.
    pub fn set(&mut self, on: bool, hours: i32, minutes: i32, now: i64, words: &dyn Fn(i32) -> String) -> Vec<AlarmOut> {
        let mut out = Vec::new();
        if on && minutes <= 0 && hours <= 0 {
            return vec![AlarmOut::Word(1402)];
        }
        if !on && self.on {
            out.push(AlarmOut::Word(1202));
        }
        self.on = on;
        self.hours = hours;
        self.minutes = minutes;
        self.when = now + hours as i64 * 3_600_000 + minutes as i64 * 60_000;
        if on {
            out.push(AlarmOut::Text(format_ints(&words(1200), &[hours, minutes])));
        }
        out
    }

    /// `Tick`: past its time it rings (word 1201 in a message box) and is
    /// off.
    pub fn tick(&mut self, now: i64, words: &dyn Fn(i32) -> String) -> Option<AlarmOut> {
        if self.on && now > self.when {
            self.on = false;
            return Some(AlarmOut::Prompt(format_ints(&words(1201), &[self.hours, self.minutes])));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(n: i32) -> String {
        match n {
            1200 => "Alarm set for %d hour(s), %d minute(s).".into(),
            1201 => "Played %02d:%02d.".into(),
            _ => format!("({n})"),
        }
    }

    /// The shipped weekday: right through 2026, a leap February a day out.
    #[test]
    fn the_weekday() {
        assert_eq!(local_time_text((2026, 10, 21, 0, 30, 12)), "Wed Oct 21 00:30:12 2026");
        assert_eq!(local_time_text((2024, 1, 1, 9, 5, 0)), "Mon Jan 1 09:05:00 2024");
        assert_eq!(local_time_text((2024, 3, 1, 0, 0, 0)), "Fri Mar 1 00:00:00 2024");
        // 2024-02-29 was a Thursday; the shipped table says Friday
        assert_eq!(local_time_text((2024, 2, 29, 0, 0, 0)), "Fri Feb 29 00:00:00 2024");
        assert_eq!(civil(1_792_542_612), (2026, 10, 21, 0, 30, 12));
        assert_eq!(civil(-1), (1969, 12, 31, 23, 59, 59));
    }

    /// "H:MM", minutes alone, "Off"; a '.' or a space refused; nothing set
    /// refused; it rings once past its time.
    #[test]
    fn the_alarm() {
        let mut a = Alarm::default();
        assert_eq!(a.command("1:30", "Off", 1000, &words), vec![AlarmOut::Text("Alarm set for 1 hour(s), 30 minute(s).".into())]);
        assert_eq!((a.on, a.when), (true, 1000 + 5_400_000));
        assert_eq!(a.command("1.30", "Off", 0, &words), vec![AlarmOut::Word(1402)]);
        assert_eq!(a.command("0", "Off", 0, &words), vec![AlarmOut::Word(1402)]);
        assert!(a.on, "still the first");
        assert_eq!(a.tick(5_401_000, &words), None);
        assert_eq!(a.tick(5_401_001, &words), Some(AlarmOut::Prompt("Played 01:30.".into())));
        assert!(!a.on && a.tick(9_999_999, &words).is_none());
        a.command("45", "Off", 0, &words);
        assert_eq!((a.hours, a.minutes), (0, 45));
        assert_eq!(a.command("off", "Off", 0, &words), vec![AlarmOut::Word(1202)]);
        assert!(!a.on);
    }

    #[test]
    fn numbers_in_words() {
        assert_eq!(format_ints("%d h %02d:%02d %%", &[3, 4, 5]), "3 h 04:05 %");
    }
}
