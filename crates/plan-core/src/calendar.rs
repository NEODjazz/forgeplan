//! Minute calendar with a fixed UTC offset. IANA/DST intentionally not implied.
use crate::{Date, Result, MAX_HORIZON};
use std::collections::{BTreeMap, BTreeSet};
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct Moment(pub(crate) i64);
impl Moment {
    pub fn parse(s: &str) -> Result<Self> {
        if s.len() != 16 || !s.is_ascii() || s.as_bytes()[10] != b'T' {
            return Err("Expected local YYYY-MM-DDTHH:MM".into());
        }
        Ok(Self(
            Date::parse(&s[..10])?.0 * 1440 + i64::from(parse_time(&s[11..], false)?),
        ))
    }
    pub fn local(self) -> String {
        format!(
            "{}T{}",
            Date(self.0.div_euclid(1440)).iso(),
            time_text(self.0.rem_euclid(1440) as u16)
        )
    }
    pub fn iso(self, offset: i16) -> String {
        format!("{}{}", self.local(), offset_text(offset))
    }
}
pub fn parse_time(s: &str, allow_end: bool) -> Result<u16> {
    let b = s.as_bytes();
    if b.len() != 5
        || b[2] != b':'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 2 || c.is_ascii_digit())
    {
        return Err("Expected HH:MM".into());
    }
    let h: u16 = s[..2].parse().map_err(|_| "Invalid hour")?;
    let m: u16 = s[3..].parse().map_err(|_| "Invalid minute")?;
    if (h < 24 && m < 60) || (allow_end && h == 24 && m == 0) {
        Ok(h * 60 + m)
    } else {
        Err("Time outside allowed range".into())
    }
}
pub fn time_text(m: u16) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}
pub fn parse_offset(s: &str) -> Result<i16> {
    if s.len() != 6 || !s.is_ascii() || !matches!(s.as_bytes()[0], b'+' | b'-') {
        return Err("Expected fixed UTC offset +03:00; IANA/DST zones are not supported".into());
    }
    let n = parse_time(&s[1..], false)?;
    if n > 840 {
        return Err("UTC offset exceeds 14 hours".into());
    }
    Ok(n as i16 * if s.starts_with('-') { -1 } else { 1 })
}
pub fn offset_text(n: i16) -> String {
    format!(
        "{}{}",
        if n < 0 { '-' } else { '+' },
        time_text(n.unsigned_abs())
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkInterval {
    pub start: u16,
    pub end: u16,
}
pub fn intervals(s: &str) -> Result<Vec<WorkInterval>> {
    if s == "-" {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for part in s.split(',') {
        let (a, b) = part
            .trim()
            .split_once('-')
            .ok_or("Expected HH:MM-HH:MM interval")?;
        result.push(WorkInterval {
            start: parse_time(a, false)?,
            end: parse_time(b, true)?,
        });
    }
    validate_intervals(&result)?;
    Ok(result)
}
pub fn intervals_text(v: &[WorkInterval]) -> String {
    if v.is_empty() {
        "-".into()
    } else {
        v.iter()
            .map(|r| format!("{}-{}", time_text(r.start), time_text(r.end)))
            .collect::<Vec<_>>()
            .join(",")
    }
}
fn validate_intervals(v: &[WorkInterval]) -> Result<()> {
    if v.len() > 24 {
        return Err("Maximum 24 intervals per day".into());
    }
    let mut end = 0;
    for r in v {
        if r.start >= r.end || r.end > 1440 || r.start < end {
            return Err("Intervals must be sorted, non-overlapping and within one day; split overnight shifts at midnight".into());
        }
        end = r.end;
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    pub start: Date,
    pub start_minute: u16,
    pub utc_offset: i16,
    pub weekly: [Vec<WorkInterval>; 7],
    pub holidays: BTreeSet<Date>,
    pub exceptions: BTreeMap<Date, Vec<WorkInterval>>,
}
impl Calendar {
    pub fn weekdays(start: Date) -> Self {
        Self {
            start,
            start_minute: 540,
            utc_offset: 0,
            weekly: std::array::from_fn(|i| {
                if i < 5 {
                    vec![WorkInterval {
                        start: 540,
                        end: 1020,
                    }]
                } else {
                    vec![]
                }
            }),
            holidays: BTreeSet::new(),
            exceptions: BTreeMap::new(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.start_minute >= 1440 || !(-840..=840).contains(&self.utc_offset) {
            return Err("Invalid calendar start time or fixed offset".into());
        }
        for v in self.weekly.iter().chain(self.exceptions.values()) {
            validate_intervals(v)?;
        }
        if self.weekly.iter().all(Vec::is_empty) && self.exceptions.values().all(Vec::is_empty) {
            return Err("Calendar has no working intervals".into());
        }
        Ok(())
    }
    pub(crate) fn on(&self, date: Date) -> &[WorkInterval] {
        if let Some(v) = self.exceptions.get(&date) {
            v
        } else if self.holidays.contains(&date) {
            &[]
        } else {
            &self.weekly[date.weekday()]
        }
    }
    pub fn work_dates(&self, count: i64) -> Result<Vec<Date>> {
        self.validate()?;
        if !(0..=100_000).contains(&count) {
            return Err("Invalid day count".into());
        }
        let mut result = Vec::new();
        let mut date = self.start;
        for _ in 0..300_000 {
            if !self.on(date).is_empty() {
                result.push(date);
                if result.len() > count as usize {
                    return Ok(result);
                }
            }
            date.0 += 1;
        }
        Err("Calendar scan limit exceeded".into())
    }
}
#[derive(Clone, Debug)]
struct Segment {
    start: i64,
    end: i64,
    base: i64,
}
#[derive(Clone, Debug)]
pub struct WorkDay {
    pub date: Date,
    pub start: i64,
    pub end: i64,
}
#[derive(Clone, Debug)]
pub struct Timeline {
    calendar: Calendar,
    segments: Vec<Segment>,
    next_day: Date,
    capacity: i64,
    scanned: u32,
}
impl Timeline {
    pub fn new(calendar: &Calendar) -> Result<Self> {
        calendar.validate()?;
        Ok(Self {
            calendar: calendar.clone(),
            segments: vec![],
            next_day: calendar.start,
            capacity: 0,
            scanned: 0,
        })
    }
    fn day(&mut self) -> Result<()> {
        if self.scanned >= 300_000 || self.next_day > Date::parse("9999-12-31")? {
            return Err("Calendar scan/date horizon exceeded".into());
        }
        let date = self.next_day;
        let anchor = self.calendar.start.0 * 1440 + i64::from(self.calendar.start_minute);
        for r in self.calendar.on(date) {
            let start = (date.0 * 1440 + i64::from(r.start)).max(anchor);
            let end = date.0 * 1440 + i64::from(r.end);
            if start < end {
                self.segments.push(Segment {
                    start,
                    end,
                    base: self.capacity,
                });
                self.capacity += end - start;
            }
        }
        self.next_day.0 += 1;
        self.scanned += 1;
        Ok(())
    }
    fn work(&mut self, n: i64) -> Result<()> {
        if !(0..=MAX_HORIZON).contains(&n) {
            return Err("Schedule exceeds supported working-minute horizon".into());
        }
        while self.capacity <= n {
            self.day()?;
        }
        Ok(())
    }
    fn wall(&mut self, m: Moment) -> Result<()> {
        while self.next_day.0 * 1440 <= m.0 {
            self.day()?;
        }
        if self.capacity > MAX_HORIZON + 1440 {
            return Err("Date constraint exceeds working-minute horizon".into());
        }
        Ok(())
    }
    pub fn day_ranges(&mut self, end: i64, limit: usize) -> Result<(Vec<WorkDay>, bool)> {
        if end == 0 {
            return Ok((vec![], false));
        }
        self.work(end - 1)?;
        let mut days: Vec<WorkDay> = Vec::new();
        for s in &self.segments {
            if s.base >= end {
                break;
            }
            let date = Date(s.start.div_euclid(1440));
            let right = (s.base + s.end - s.start).min(end);
            if let Some(last) = days.last_mut().filter(|last| last.date == date) {
                last.end = right;
            } else {
                if days.len() == limit {
                    return Ok((days, true));
                }
                days.push(WorkDay {
                    date,
                    start: s.base,
                    end: right,
                });
            }
        }
        Ok((days, false))
    }
    pub fn start_at(&mut self, n: i64) -> Result<Moment> {
        self.work(n)?;
        let i = self
            .segments
            .partition_point(|s| s.base + s.end - s.start <= n);
        let s = &self.segments[i];
        Ok(Moment(s.start + n - s.base))
    }
    pub fn finish_at(&mut self, n: i64) -> Result<Moment> {
        if n == 0 {
            return self.start_at(0);
        }
        self.work(n - 1)?;
        let i = self
            .segments
            .partition_point(|s| s.base + s.end - s.start < n);
        let s = &self.segments[i];
        Ok(Moment(s.start + n - s.base))
    }
    pub fn start_ceil(&mut self, m: Moment) -> Result<i64> {
        self.wall(m)?;
        loop {
            let i = self.segments.partition_point(|s| s.end <= m.0);
            if let Some(s) = self.segments.get(i) {
                let n = s.base + (m.0 - s.start).max(0);
                if n > MAX_HORIZON {
                    return Err("Constraint exceeds horizon".into());
                }
                return Ok(n);
            }
            self.day()?;
        }
    }
    pub fn start_floor(&mut self, m: Moment) -> Result<i64> {
        self.wall(m)?;
        let i = self.segments.partition_point(|s| s.start <= m.0);
        if i == 0 {
            return Ok(-1);
        }
        let s = &self.segments[i - 1];
        Ok(s.base + (m.0.min(s.end - 1) - s.start))
    }
    pub fn finish_floor(&mut self, m: Moment) -> Result<i64> {
        self.wall(m)?;
        let i = self.segments.partition_point(|s| s.start < m.0);
        if i == 0 {
            return if m.0 >= self.calendar.start.0 * 1440 + i64::from(self.calendar.start_minute) {
                Ok(0)
            } else {
                Ok(-1)
            };
        }
        let s = &self.segments[i - 1];
        Ok(s.base + (m.0.min(s.end) - s.start))
    }
    pub fn finish_ceil(&mut self, m: Moment) -> Result<i64> {
        let n = self.finish_floor(m)?;
        if n < 0 {
            return Ok(0);
        }
        if self.finish_at(n)?.0 < m.0 {
            Ok(n + 1)
        } else {
            Ok(n)
        }
    }
}

impl TryFrom<String> for Moment {
    type Error = String;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}
impl From<Moment> for String {
    fn from(value: Moment) -> Self {
        value.local()
    }
}
