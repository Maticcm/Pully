#[derive(Debug, PartialEq)]
pub struct ParsedProgress {
    pub percent: f64,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
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
        .ok()?
        .clamp(0.0, 100.0);
    Some(ParsedProgress {
        percent,
        downloaded_bytes: optional_number(fields.get(1)),
        total_bytes: optional_number(fields.get(2)).or_else(|| optional_number(fields.get(3))),
        speed: fields
            .get(4)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty() && v != "NA"),
        eta: fields
            .get(5)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty() && v != "NA"),
    })
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
}
