pub const REPOSITORY_URL: &str = "https://github.com/divyanshudhruv/slate";
pub const RELEASES_URL: &str = "https://github.com/divyanshudhruv/slate/releases/latest";
pub const LATEST_API_URL: &str =
    "https://api.github.com/repos/divyanshudhruv/slate/releases/latest";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
// Throttle for repeated menu clicks; also keeps API calls rare.
pub const CHECK_COOLDOWN_SECS: i64 = 5;

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

/// Release assets follow `release.yml` naming: slate-v{tag}-{arch}.exe.
pub fn download_url(tag: &str, arch: &str) -> String {
    format!(
        "https://github.com/divyanshudhruv/slate/releases/download/v{tag}/slate-v{tag}-{arch}.exe"
    )
}

/// What launching Slate reveals about the registered-exe record
/// (`%USERPROFILE%\.burntfern\slate\install.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Startup {
    /// No record, or it points at this very exe: carry on.
    Continue,
    /// A different copy is running right now; focus it and quit.
    AlreadyRunning { pid: u32 },
    /// The record names a different file that is not running: offer to
    /// remove it if its version is older than the copy just launched.
    StaleCopy { path: String, version: String },
}

/// `registered` is `(path, version, pid)` from the record, `current_exe` is
/// the running binary, and `other_alive` says whether the recorded pid is
/// still a live process (liveness itself is a Win32 concern, kept out here).
pub fn classify_startup(
    registered: Option<(&str, &str, u32)>,
    current_exe: &str,
    current_version: &str,
    other_alive: bool,
) -> Startup {
    let Some((path, version, pid)) = registered else {
        return Startup::Continue;
    };
    if path.eq_ignore_ascii_case(current_exe) {
        return Startup::Continue;
    }
    if other_alive {
        return Startup::AlreadyRunning { pid };
    }
    if is_newer(current_version, version) {
        // The registered copy is the newer one, so running this old exe is a
        // deliberate choice - allow it and let the record follow the last run.
        return Startup::Continue;
    }
    Startup::StaleCopy {
        path: path.to_string(),
        version: version.to_string(),
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
    fn download_url_matches_release_asset_names() {
        assert_eq!(
            download_url("0.2.0", "x64"),
            "https://github.com/divyanshudhruv/slate/releases/download/v0.2.0/slate-v0.2.0-x64.exe"
        );
        assert_eq!(
            download_url("v0.2.0", "arm64"),
            "https://github.com/divyanshudhruv/slate/releases/download/vv0.2.0/slate-vv0.2.0-arm64.exe"
        );
    }

    #[test]
    fn first_launch_or_same_path_continues() {
        assert_eq!(
            classify_startup(None, "C:\\a\\slate.exe", "0.1.0", false),
            Startup::Continue
        );
        // Path compare is case-insensitive: Windows paths are.
        assert_eq!(
            classify_startup(
                Some(("c:\\a\\SLATE.exe", "0.1.0", 7)),
                "C:\\a\\slate.exe",
                "0.1.0",
                true
            ),
            Startup::Continue
        );
    }

    #[test]
    fn live_other_copy_wins_the_session() {
        assert_eq!(
            classify_startup(
                Some(("C:\\a\\slate.exe", "0.1.0", 4242)),
                "C:\\Downloads\\slate.exe",
                "0.1.1",
                true
            ),
            Startup::AlreadyRunning { pid: 4242 }
        );
    }

    #[test]
    fn dead_older_copy_is_offered_for_cleanup() {
        assert_eq!(
            classify_startup(
                Some(("C:\\a\\slate-v0.1.0-x64.exe", "0.1.0", 4242)),
                "C:\\Downloads\\slate-v0.1.1-x64.exe",
                "0.1.1",
                false
            ),
            Startup::StaleCopy {
                path: "C:\\a\\slate-v0.1.0-x64.exe".into(),
                version: "0.1.0".into(),
            }
        );
        // Same version in another folder still leaves a stray copy behind.
        assert!(matches!(
            classify_startup(
                Some(("C:\\a\\slate.exe", "0.1.0", 1)),
                "C:\\b\\slate.exe",
                "0.1.0",
                false
            ),
            Startup::StaleCopy { .. }
        ));
    }

    #[test]
    fn dead_newer_copy_is_a_deliberate_rollback() {
        assert_eq!(
            classify_startup(
                Some(("C:\\a\\slate.exe", "0.2.0", 4242)),
                "C:\\a\\old\\slate.exe",
                "0.1.0",
                false
            ),
            Startup::Continue
        );
        // Unparseable recorded version behaves like no claim at all.
        assert!(matches!(
            classify_startup(
                Some(("C:\\a\\slate.exe", "garbage", 1)),
                "C:\\b\\slate.exe",
                "0.1.0",
                false
            ),
            Startup::StaleCopy { .. }
        ));
    }

    #[test]
    fn tag_comes_from_release_json() {
        let body = r#"{"tag_name":"v0.2.0","name":"Slate 0.2.0"}"#;
        assert_eq!(parse_release_tag(body).as_deref(), Some("0.2.0"));
        assert_eq!(parse_release_tag("{}"), None);
        assert_eq!(parse_release_tag("not json"), None);
    }
}
