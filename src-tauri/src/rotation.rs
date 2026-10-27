use crate::models::Settings;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShuffleBag {
    pub known: Vec<String>,
    pub remaining: Vec<String>,
}
impl ShuffleBag {
    pub fn next(&mut self, ids: &[String], previous: Option<&str>) -> Option<String> {
        let available: HashSet<_> = ids.iter().cloned().collect();
        self.remaining.retain(|id| available.contains(id));
        let mut changed = false;
        for id in ids {
            if !self.known.contains(id) && !self.remaining.contains(id) {
                self.remaining.push(id.clone());
                changed = true;
            }
        }
        self.known = ids.to_vec();
        if self.remaining.is_empty() {
            self.remaining = ids.to_vec();
            changed = true;
        }
        if changed {
            self.remaining.shuffle(&mut rand::thread_rng());
        }
        if self.remaining.len() > 1 && self.remaining.last().map(String::as_str) == previous {
            let last = self.remaining.len() - 1;
            self.remaining.swap(0, last);
        }
        self.remaining.pop()
    }
}

pub fn schedule_allows(settings: &Settings, day: u32, minute: u32) -> bool {
    if !settings.schedule_enabled {
        return true;
    }
    fn minutes(time: &str) -> Option<u32> {
        let (h, m) = time.split_once(':')?;
        let h: u32 = h.parse().ok()?;
        let m: u32 = m.parse().ok()?;
        (h < 24 && m < 60).then_some(h * 60 + m)
    }
    let (Some(start), Some(end)) = (minutes(&settings.start_time), minutes(&settings.end_time))
    else {
        return false;
    };
    if start == end {
        return settings.days.contains(&day);
    }
    if start < end {
        return settings.days.contains(&day) && minute >= start && minute < end;
    }
    (minute >= start && settings.days.contains(&day))
        || (minute < end && settings.days.contains(&((day + 6) % 7)))
}

pub fn choose_index(length: usize, previous: Option<usize>, shuffle: bool) -> Option<usize> {
    if length == 0 {
        return None;
    }
    if !shuffle {
        return Some(previous.map_or(0, |p| (p + 1) % length));
    }
    if length == 1 {
        return Some(0);
    }
    if let Some(previous) = previous.filter(|p| *p < length) {
        let next = rand::thread_rng().gen_range(0..length - 1);
        Some(if next >= previous { next + 1 } else { next })
    } else {
        Some(rand::thread_rng().gen_range(0..length))
    }
}
