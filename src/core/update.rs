pub const REPOSITORY_URL: &str = "https://github.com/burntfern/slate";
pub const RELEASES_URL: &str = "https://github.com/burntfern/slate/releases/latest";
pub const LATEST_API_URL: &str = "https://api.github.com/repos/burntfern/slate/releases/latest";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CHECK_COOLDOWN_SECS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(String),
    Failed,
}

impl UpdateState {
    pub fn from_cached(current: &str, cached: Option<&str>) -> Self {
        match cached {
            Some(version) if is_newer(current, version) => Self::Available(version.to_string()),
            Some(_) => Self::UpToDate,
            None => Self::Idle,
        }
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let mut parts = text.split('.');
    let major = leading_number(parts.next()?)?;
    let minor = parts.next().and_then(leading_number).unwrap_or(0);
    let patch = parts.next().and_then(leading_number).unwrap_or(0);
    Some((major, minor, patch))
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

pub fn parse_release_tag(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let tag = value.get("tag_name")?.as_str()?.trim();
    let tag = tag.trim_start_matches(['v', 'V']);
    (!tag.is_empty()).then(|| tag.to_string())
}

fn leading_number(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_prefixed_versions() {
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1"), Some((1, 0, 0)));
        assert_eq!(parse_version("0.1.0-beta"), Some((0, 1, 0)));
        assert_eq!(parse_version("nonsense"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn newer_compares_each_component() {
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.0", "1.0.0"));
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(!is_newer("0.2.0", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "nonsense"));
    }

    #[test]
    fn cached_state_uses_version_compare() {
        assert_eq!(
            UpdateState::from_cached("0.1.0", Some("0.2.0")),
            UpdateState::Available("0.2.0".into())
        );
        assert_eq!(
            UpdateState::from_cached("0.1.0", Some("0.1.0")),
            UpdateState::UpToDate
        );
        assert_eq!(UpdateState::from_cached("0.1.0", None), UpdateState::Idle);
    }

    #[test]
    fn tag_comes_from_release_json() {
        let body = r#"{"tag_name":"v0.2.0","name":"Slate 0.2.0"}"#;
        assert_eq!(parse_release_tag(body).as_deref(), Some("0.2.0"));
        assert_eq!(parse_release_tag("{}"), None);
        assert_eq!(parse_release_tag("not json"), None);
    }
}
