use std::time::Duration;

pub const PLAN_TEMPLATE: &str = "before_dl:PULLY_PLAN|%(.{format_id,filesize,filesize_approx,requested_formats,playlist_index,n_entries})j";
pub const PROGRESS_TEMPLATE: &str = "download:PULLY_PROGRESS|%(progress._percent_str)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress._speed_str)s|%(progress._eta_str)s|%(info.format_id)s|%(progress.status)s|%(progress.speed)s";

#[derive(Debug, PartialEq)]
pub struct ParsedProgress {
    pub percent: f64,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub format_id: Option<String>,
    pub finished: bool,
    pub bytes_per_second: Option<f64>,
}

fn optional_number(value: Option<&&str>) -> Option<u64> {
    value.and_then(|v| v.trim().parse().ok()).filter(|v| *v > 0)
}

pub fn parse_progress(line: &str) -> Option<ParsedProgress> {
    let data = line.strip_prefix("PULLY_PROGRESS|")?;
    let fields: Vec<_> = data.split('|').collect();
    if fields.len() < 5 {
        return None;
    }
    let percent = fields[0]
        .trim()
        .trim_end_matches('%')
        .trim()
        .parse::<f64>()
        .ok()?;
    if !percent.is_finite() {
        return None;
    }
    Some(ParsedProgress {
        percent: percent.clamp(0.0, 100.0),
        downloaded_bytes: fields.get(1).and_then(|v| v.trim().parse().ok()),
        total_bytes: optional_number(fields.get(2)).or_else(|| optional_number(fields.get(3))),
        speed: fields
            .get(4)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty() && v != "NA"),
        eta: fields
            .get(5)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty() && v != "NA"),
        format_id: fields
            .get(6)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty() && v != "NA"),
        finished: fields.get(7).is_some_and(|v| v.trim() == "finished"),
        bytes_per_second: fields
            .get(8)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0),
    })
}

#[derive(Default)]
struct Stream {
    id: String,
    size: Option<u64>,
    downloaded: u64,
    percent: f64,
    label: &'static str,
}

#[derive(Default)]
pub struct ProgressTracker {
    streams: Vec<Stream>,
    percent: f64,
    rate: Option<f64>,
    first_sample: Option<Duration>,
    last_sample: Option<Duration>,
    last_display: Option<Duration>,
    speed: Option<String>,
    eta: Option<String>,
    item: String,
}

pub struct DisplayProgress {
    pub percent: f64,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub phase: String,
}

impl ProgressTracker {
    pub fn begin(&mut self, line: &str) -> bool {
        let Some(json) = line.strip_prefix("PULLY_PLAN|") else {
            return false;
        };
        let Ok(plan) = serde_json::from_str::<serde_json::Value>(json) else {
            return false;
        };
        let formats = plan.get("requested_formats").and_then(|v| v.as_array());
        let entries = formats
            .filter(|v| !v.is_empty())
            .map(|v| v.iter().collect::<Vec<_>>())
            .unwrap_or_else(|| vec![&plan]);
        let streams = entries
            .into_iter()
            .filter_map(|entry| {
                Some(Stream {
                    id: entry.get("format_id")?.as_str()?.to_string(),
                    size: entry
                        .get("filesize")
                        .and_then(|v| v.as_u64())
                        .filter(|v| *v > 0)
                        .or_else(|| {
                            entry
                                .get("filesize_approx")
                                .and_then(|v| v.as_u64())
                                .filter(|v| *v > 0)
                        }),
                    label: if entry.get("vcodec").and_then(|v| v.as_str()) == Some("none") {
                        "audio"
                    } else if entry.get("acodec").and_then(|v| v.as_str()) == Some("none") {
                        "video"
                    } else {
                        "media"
                    },
                    ..Stream::default()
                })
            })
            .collect::<Vec<_>>();
        if streams.is_empty() {
            return false;
        }
        *self = Self {
            streams,
            ..Self::default()
        };
        if let Some(index) = plan.get("playlist_index").and_then(|v| v.as_u64()) {
            self.item = match plan.get("n_entries").and_then(|v| v.as_u64()) {
                Some(total) => format!("Item {index}/{total} · "),
                None => format!("Item {index} · "),
            };
        }
        true
    }

    pub fn update(&mut self, value: ParsedProgress, now: Duration) -> Option<DisplayProgress> {
        // Old providers without a plan still get stable, bounded progress.
        if self.streams.is_empty() {
            self.streams.push(Stream {
                id: value.format_id.clone().unwrap_or_default(),
                label: "media",
                ..Stream::default()
            });
        }
        let index = value
            .format_id
            .as_ref()
            .and_then(|id| self.streams.iter().position(|s| &s.id == id))
            .or_else(|| (self.streams.len() == 1).then_some(0))?;
        let stream = &mut self.streams[index];
        if let Some(size) = value.total_bytes {
            stream.size = Some(size);
        }
        stream.downloaded = stream.downloaded.max(value.downloaded_bytes.unwrap_or(0));
        stream.percent = stream
            .percent
            .max(if value.finished { 100.0 } else { value.percent });
        if value.finished {
            stream.size = value.downloaded_bytes.or(stream.size);
        }
        let total = self
            .streams
            .iter()
            .try_fold(0u64, |sum, s| sum.checked_add(s.size?));
        let downloaded = self
            .streams
            .iter()
            .fold(0u64, |sum, s| sum.saturating_add(s.downloaded));
        let fraction = match total.filter(|v| *v > 0) {
            Some(total) => {
                self.streams
                    .iter()
                    .map(|s| s.size.unwrap_or(0) as f64 * s.percent / 100.0)
                    .sum::<f64>()
                    / total as f64
            }
            None => {
                self.streams.iter().map(|s| s.percent).sum::<f64>()
                    / (100.0 * self.streams.len() as f64)
            }
        };
        self.percent = self.percent.max((fraction * 100.0).min(99.0));
        if let Some(rate) = value.bytes_per_second {
            let first = *self.first_sample.get_or_insert(now);
            let elapsed = self
                .last_sample
                .map(|last| now.saturating_sub(last).as_secs_f64())
                .unwrap_or(0.0);
            self.rate = Some(match self.rate {
                Some(previous) => previous + (rate - previous) * (1.0 - (-elapsed / 5.0).exp()),
                None => rate,
            });
            self.last_sample = Some(now);
            if now.saturating_sub(first) >= Duration::from_secs(2)
                && self
                    .last_display
                    .is_none_or(|last| now.saturating_sub(last) >= Duration::from_secs(1))
            {
                let smoothed = self.rate.unwrap_or(rate);
                self.speed = Some(format_speed(smoothed));
                self.eta = total.filter(|total| *total > downloaded).map(|total| {
                    let seconds = ((total - downloaded) as f64 / smoothed)
                        .ceil()
                        .min(604800.0) as u64;
                    let seconds = if seconds >= 10 {
                        seconds.div_ceil(5) * 5
                    } else {
                        seconds.max(1)
                    };
                    format!("~{}", format_eta(seconds))
                });
                self.last_display = Some(now);
            }
        } else if self
            .last_sample
            .is_some_and(|last| now.saturating_sub(last) >= Duration::from_secs(3))
        {
            self.speed = None;
            self.eta = None;
        }
        if total.is_none_or(|total| total <= downloaded) {
            self.eta = None;
        }
        Some(DisplayProgress {
            percent: self.percent,
            downloaded_bytes: Some(downloaded),
            total_bytes: total,
            speed: self.speed.clone(),
            eta: self.eta.clone(),
            phase: if self.streams.len() > 1 {
                format!(
                    "{}Downloading {} ({}/{})",
                    self.item,
                    self.streams[index].label,
                    index + 1,
                    self.streams.len()
                )
            } else {
                format!("{}Downloading", self.item)
            },
        })
    }
}

fn format_speed(rate: f64) -> String {
    let (value, unit) = if rate >= 1024.0 * 1024.0 {
        (rate / (1024.0 * 1024.0), "MiB/s")
    } else if rate >= 1024.0 {
        (rate / 1024.0, "KiB/s")
    } else {
        (rate, "B/s")
    };
    format!("{value:.1} {unit}")
}

fn format_eta(seconds: u64) -> String {
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_machine_progress() {
        let p = parse_progress("PULLY_PROGRESS| 42.5%|425|1000|NA|2.1MiB/s|00:04").unwrap();
        assert_eq!(p.percent, 42.5);
        assert_eq!(p.total_bytes, Some(1000));
        assert_eq!(p.eta.as_deref(), Some("00:04"));
    }
    #[test]
    fn ignores_unrelated_output() {
        assert!(parse_progress("[download] 10%").is_none());
    }

    fn sample(
        format: &str,
        downloaded: u64,
        total: u64,
        speed: u64,
        finished: bool,
    ) -> ParsedProgress {
        parse_progress(&format!(
            "PULLY_PROGRESS|{}%|{downloaded}|{total}|NA|NA|NA|{format}|{}|{speed}",
            downloaded as f64 * 100.0 / total as f64,
            if finished { "finished" } else { "downloading" }
        ))
        .unwrap()
    }

    fn two_streams() -> ProgressTracker {
        let mut tracker = ProgressTracker::default();
        assert!(tracker.begin(r#"PULLY_PLAN|{"requested_formats":[{"format_id":"v","filesize":9000,"acodec":"none"},{"format_id":"a","filesize":1000,"vcodec":"none"}]}"#));
        tracker
    }

    #[test]
    fn combines_video_and_audio_without_restarting_or_finishing_early() {
        let mut tracker = two_streams();
        let video = tracker
            .update(sample("v", 9000, 9000, 1000, true), Duration::from_secs(3))
            .unwrap();
        assert_eq!(video.percent, 90.0);
        assert_eq!(video.total_bytes, Some(10000));
        let audio = tracker
            .update(sample("a", 100, 1000, 1000, false), Duration::from_secs(4))
            .unwrap();
        assert_eq!(audio.percent, 91.0);
        assert_eq!(audio.downloaded_bytes, Some(9100));
        assert_eq!(audio.phase, "Downloading audio (2/2)");
        let done = tracker
            .update(sample("a", 1000, 1000, 1000, true), Duration::from_secs(5))
            .unwrap();
        assert_eq!(done.percent, 99.0);
        assert!(done.eta.is_none());
    }

    #[test]
    fn smooths_rate_spikes_and_throttles_estimates_after_warmup() {
        let mut tracker = ProgressTracker::default();
        assert!(tracker
            .update(sample("v", 1000, 100000, 1000, false), Duration::ZERO)
            .unwrap()
            .eta
            .is_none());
        tracker.update(
            sample("v", 2000, 100000, 1000, false),
            Duration::from_secs(1),
        );
        let steady = tracker
            .update(
                sample("v", 3000, 100000, 1000, false),
                Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(steady.speed.as_deref(), Some("1000.0 B/s"));
        assert_eq!(steady.eta.as_deref(), Some("~01:40"));
        let spike = tracker
            .update(
                sample("v", 4000, 100000, 100000, false),
                Duration::from_millis(2250),
            )
            .unwrap();
        assert_eq!(spike.eta, steady.eta);
        assert_eq!(spike.speed, steady.speed);
        let settled = tracker
            .update(
                sample("v", 5000, 100000, 1000, false),
                Duration::from_secs(3),
            )
            .unwrap();
        assert!(settled.speed.as_deref() != Some("97.7 KiB/s"));
    }

    #[test]
    fn keeps_progress_monotonic_when_fragment_size_estimates_change() {
        let mut tracker = ProgressTracker::default();
        let first = tracker
            .update(sample("v", 5000, 10000, 1000, false), Duration::ZERO)
            .unwrap();
        let revised = tracker
            .update(
                sample("v", 6000, 20000, 1000, false),
                Duration::from_secs(1),
            )
            .unwrap();
        assert!(revised.percent >= first.percent);
        assert_eq!(revised.total_bytes, Some(20000));
    }

    #[test]
    fn unknown_stream_sizes_do_not_show_a_made_up_eta() {
        let mut tracker = ProgressTracker::default();
        tracker.begin(r#"PULLY_PLAN|{"requested_formats":[{"format_id":"v"},{"format_id":"a"}]}"#);
        tracker.update(sample("v", 4000, 9000, 1000, false), Duration::ZERO);
        let result = tracker
            .update(sample("v", 9000, 9000, 1000, true), Duration::from_secs(3))
            .unwrap();
        assert_eq!(result.percent, 50.0);
        assert!(result.eta.is_none());
        assert!(result.total_bytes.is_none());
    }

    #[test]
    fn new_playlist_item_clears_previous_streams_and_estimates() {
        let mut tracker = two_streams();
        tracker.update(sample("v", 9000, 9000, 1000, true), Duration::ZERO);
        tracker.begin(
            r#"PULLY_PLAN|{"format_id":"muxed","filesize":5000,"playlist_index":2,"n_entries":3}"#,
        );
        let next = tracker
            .update(
                sample("muxed", 500, 5000, 1000, false),
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(next.percent, 10.0);
        assert_eq!(next.phase, "Item 2/3 · Downloading");
        assert!(next.eta.is_none());
    }

    #[test]
    fn rejects_nonfinite_progress_and_speed() {
        assert!(parse_progress("PULLY_PROGRESS|NaN%|1|2|NA|NA|NA").is_none());
        assert!(parse_progress("PULLY_PROGRESS|inf%|1|2|NA|NA|NA").is_none());
        let p = parse_progress("PULLY_PROGRESS|0%|0|2|NA|NA|NA|v|downloading|NaN").unwrap();
        assert_eq!(p.downloaded_bytes, Some(0));
        assert!(p.bytes_per_second.is_none());
    }
}
