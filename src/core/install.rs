use std::path::{Path, PathBuf};

use super::update::{is_newer, parse_version};

pub const OLD_NAME: &str = "slate.exe.old";
pub const STAGED_NAME: &str = "slate-staged.exe";
pub const CANONICAL_NAME: &str = "slate.exe";
pub const STAGING_PREFIX: &str = "slate-";

pub fn backup_path(canonical: &Path) -> PathBuf {
    canonical.with_file_name(OLD_NAME)
}

pub fn needs_backup(installed: bool, backup_exists: bool) -> bool {
    installed && !backup_exists
}

const ARCHES: &[&str] = &["x64", "arm64"];

const DEV_OUTPUT_DIRS: &[&str] = &[
    "debug", "release", "deps", "test", "benches", "examples", "doc",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Dev,
    Run { exe: String, canonical: bool },
    FocusRunning { pid: u32 },
    Bootstrap { source: String },
    Update { candidate: String },
    Defer { canonical: String },
}

#[derive(Debug, Clone, Copy)]
pub struct Situation<'a> {
    pub exe: &'a str,
    pub canonical: &'a str,
    pub canonical_exists: bool,
    pub record: Option<Record<'a>>,
    pub record_canonical_alive: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    pub path: &'a str,
    pub version: &'a str,
    pub pid: u32,
}

pub fn classify(s: Situation) -> Plan {
    if s.exe.is_empty() {
        return Plan::Dev;
    }
    if dev_build(s.exe) {
        return Plan::Dev;
    }
    if same_path(s.exe, s.canonical) {
        if s.record_canonical_alive {
            return Plan::FocusRunning {
                pid: s.record.map(|r| r.pid).unwrap_or(0),
            };
        }
        return Plan::Run {
            exe: s.exe.to_string(),
            canonical: true,
        };
    }
    let Some((candidate_version, _)) = parse_release_filename(Path::new(s.exe)) else {
        return Plan::Run {
            exe: s.exe.to_string(),
            canonical: false,
        };
    };
    if !s.canonical_exists {
        return Plan::Bootstrap {
            source: s.exe.to_string(),
        };
    }
    let installed_version = s
        .record
        .filter(|r| same_path(r.path, s.canonical))
        .map(|r| r.version);
    let candidate_wins = match installed_version {
        None => true,
        Some(installed) => is_newer(installed, &candidate_version),
    };
    if candidate_wins {
        Plan::Update {
            candidate: s.exe.to_string(),
        }
    } else {
        Plan::Defer {
            canonical: s.canonical.to_string(),
        }
    }
}

pub fn dev_build(path: &str) -> bool {
    let comps: Vec<String> = Path::new(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let Some(pos) = comps.iter().position(|c| c == "target") else {
        return false;
    };
    comps[pos + 1..]
        .iter()
        .any(|c| DEV_OUTPUT_DIRS.contains(&c.as_str()))
}

pub fn same_path(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

pub fn parse_release_filename(path: &Path) -> Option<(String, String)> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe")?.to_string();
    let rest = stem.strip_prefix("slate-")?;
    let rest = rest.strip_prefix(['v', 'V']).unwrap_or(rest);
    let (version_part, arch) = rest.split_once('-')?;
    if !ARCHES.contains(&arch) {
        return None;
    }
    if version_part.contains('-') {
        return None;
    }
    let version = version_part.trim();
    if version.split('.').count() != 3 {
        return None;
    }
    parse_version(version)
        .is_some()
        .then_some((version.to_string(), arch.to_string()))
}

pub fn checksum_for<'a>(sums: &'a str, file: &str) -> Option<&'a str> {
    sums.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (hash, name) = line.split_once("  ")?;
            let name = name.trim();
            if name.eq_ignore_ascii_case(file)
                && hash.len() == 64
                && hash.chars().all(|c| c.is_ascii_hexdigit())
            {
                Some(hash)
            } else {
                None
            }
        })
        .next()
}

pub fn staged_download_name(version: &str, arch: &str) -> String {
    format!("{STAGING_PREFIX}update-v{version}-{arch}.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANON: &str = "C:\\Users\\a\\.burntfern\\slate\\slate.exe";
    const DOWNLOAD_1: &str = "C:\\Users\\a\\Downloads\\slate-v0.1.0-x64.exe";
    const DOWNLOAD_2: &str = "C:\\Users\\a\\Downloads\\slate-v0.2.0-x64.exe";

    fn sit<'a>(
        exe: &'a str,
        record: Option<(&'a str, &'a str, u32)>,
        canonical_exists: bool,
        alive: bool,
    ) -> Situation<'a> {
        Situation {
            exe,
            canonical: CANON,
            canonical_exists,
            record: record.map(|(path, version, pid)| Record { path, version, pid }),
            record_canonical_alive: alive,
        }
    }

    #[test]
    fn the_backup_lives_beside_the_exe_it_replaces() {
        assert_eq!(
            backup_path(Path::new(CANON)),
            Path::new("C:\\Users\\a\\.burntfern\\slate\\slate.exe.old")
        );
        assert_eq!(
            backup_path(Path::new(CANON)).parent(),
            Path::new(CANON).parent()
        );
    }

    #[test]
    fn a_backup_is_kept_only_when_there_is_one_to_keep() {
        assert!(!needs_backup(false, false));
        assert!(needs_backup(true, false));
        assert!(!needs_backup(true, true));
        assert!(!needs_backup(false, true));
    }

    #[test]
    fn dev_build_recognizes_output_dirs_only() {
        assert!(dev_build("C:\\x\\target\\debug\\slate.exe"));
        assert!(dev_build("c:\\TARGET\\Release\\slate.exe"));
        assert!(dev_build(
            "C:\\x\\target\\aarch64-pc-windows-msvc\\release\\slate.exe"
        ));
        assert!(!dev_build(CANON));
        assert!(!dev_build(DOWNLOAD_1));
        assert!(!dev_build("C:\\x\\target\\notes\\slate.exe"));
        assert!(!dev_build(""));
    }

    #[test]
    fn canonical_run_and_single_instance() {
        assert_eq!(
            classify(sit(
                CANON,
                Some(("C:\\a\\dead.exe", "0.1.0", 5)),
                true,
                false
            )),
            Plan::Run {
                exe: CANON.into(),
                canonical: true
            }
        );
        assert_eq!(
            classify(sit(CANON, Some((CANON, "0.1.0", 4242)), true, true)),
            Plan::FocusRunning { pid: 4242 }
        );
    }

    #[test]
    fn release_download_bootstraps_when_no_install_exists() {
        assert_eq!(
            classify(sit(DOWNLOAD_1, None, false, false)),
            Plan::Bootstrap {
                source: DOWNLOAD_1.into()
            }
        );
        assert_eq!(
            classify(sit(
                DOWNLOAD_1,
                Some(("D:\\repo\\target\\debug\\slate.exe", "0.2.0", 8828)),
                false,
                false
            )),
            Plan::Bootstrap {
                source: DOWNLOAD_1.into()
            }
        );
    }

    #[test]
    fn renamed_or_dev_copies_stay_portable() {
        assert_eq!(
            classify(sit(
                "C:\\Users\\a\\Downloads\\Slate.exe",
                None,
                false,
                false
            )),
            Plan::Run {
                exe: "C:\\Users\\a\\Downloads\\Slate.exe".into(),
                canonical: false
            }
        );
        assert_eq!(
            classify(sit("C:\\a\\slate.exe.old", None, true, false)),
            Plan::Run {
                exe: "C:\\a\\slate.exe.old".into(),
                canonical: false
            }
        );
    }

    #[test]
    fn manual_download_updates_strictly_newer_install() {
        let old_record = Some((CANON, "0.1.0", 999));
        assert_eq!(
            classify(sit(DOWNLOAD_2, old_record, true, false)),
            Plan::Update {
                candidate: DOWNLOAD_2.into()
            }
        );
        assert_eq!(
            classify(sit(DOWNLOAD_1, old_record, true, false)),
            Plan::Defer {
                canonical: CANON.into()
            }
        );
    }

    #[test]
    fn stale_or_missing_record_lets_candidate_update() {
        assert_eq!(
            classify(sit(
                DOWNLOAD_2,
                Some(("D:\\repo\\target\\debug\\slate.exe", "0.2.0", 8828)),
                true,
                false
            )),
            Plan::Update {
                candidate: DOWNLOAD_2.into()
            }
        );
        assert_eq!(
            classify(sit(DOWNLOAD_2, None, true, false)),
            Plan::Update {
                candidate: DOWNLOAD_2.into()
            }
        );
    }

    #[test]
    fn release_filename_parsing_is_strict() {
        let ok = Path::new("C:\\d\\slate-v1.2.3-arm64.exe");
        assert_eq!(
            parse_release_filename(ok),
            Some(("1.2.3".to_string(), "arm64".to_string()))
        );
        let ok_no_v = Path::new("slate-1.0.0-x64.EXE");
        assert_eq!(
            parse_release_filename(ok_no_v),
            Some(("1.0.0".to_string(), "x64".to_string()))
        );
        for bad in [
            "slate.exe",
            "slate.exe.old",
            "slate-v1.2-x64.exe",
            "slate-v1.2.3.exe",
            "slate-v1.2.3-x86.exe",
            "slate-v1.2.3-x64.exe.bak",
            "myapp-v1.0.0-x64.exe",
            "slate-v1.2.3-x64-extra.exe",
            "slate-vbeta-x64.exe",
        ] {
            assert_eq!(parse_release_filename(Path::new(bad)), None, "{bad}");
        }
    }

    #[test]
    fn checksum_matches_release_yml_format() {
        let h1 = "a".repeat(64);
        let h2 = "b".repeat(64);
        let sums = format!("{h1}  slate-v0.1.0-x64.exe\n{h2}  slate-v0.1.0-arm64.exe\n");
        assert_eq!(
            checksum_for(&sums, "slate-v0.1.0-x64.exe"),
            Some(h1.as_str())
        );
        assert_eq!(
            checksum_for(&sums, "slate-v0.1.0-arm64.exe"),
            Some(h2.as_str())
        );
        assert_eq!(checksum_for(&sums, "slate-v9.9.9-x64.exe"), None);
        assert_eq!(checksum_for("not a sums file", "slate.exe"), None);
        let bad = "short  slate-v0.1.0-x64.exe\nzzz  slate-v0.1.0-arm64.exe\n";
        assert_eq!(checksum_for(bad, "slate-v0.1.0-x64.exe"), None);
    }
}
