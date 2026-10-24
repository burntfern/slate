use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default = "theme_default")]
    pub theme: String,
    #[serde(default = "accent_default")]
    pub accent: String,
    #[serde(default)]
    pub tabs: Tabs,
    #[serde(default)]
    pub updates: Updates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tabs {
    #[serde(default = "yes")]
    pub icons: bool,
    #[serde(default = "yes")]
    pub count_badge: bool,
    #[serde(default = "yes")]
    pub hover_preview: bool,
    #[serde(default = "yes")]
    pub show_bar: bool,
}

impl Default for Tabs {
    fn default() -> Self {
        Self {
            icons: true,
            count_badge: true,
            hover_preview: true,
            show_bar: true,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            theme: theme_default(),
            accent: accent_default(),
            tabs: Tabs::default(),
            updates: Updates::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSet {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Updates {
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub checked_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRecord {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedTab {
    #[serde(default)]
    pub exe_path: String,
    #[serde(default)]
    pub exe_name: String,
    #[serde(default)]
    pub title_hint: String,
    #[serde(default)]
    pub accent: Option<usize>,
}

#[derive(Serialize)]
pub struct AppSettings<'a> {
    pub schema_version: u32,
    pub tabs: &'a Tabs,
    pub updates: &'a Updates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSets {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub sets: Vec<SavedSet>,
}

impl Default for SavedSets {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            sets: Vec::new(),
        }
    }
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}
fn theme_default() -> String {
    "system".to_string()
}
fn accent_default() -> String {
    "blue".to_string()
}
fn yes() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_all_on() {
        let s = Settings::default();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert!(s.tabs.icons && s.tabs.count_badge && s.tabs.hover_preview);
    }

    #[test]
    fn settings_round_trip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, s.theme);
        assert_eq!(back.tabs.icons, s.tabs.icons);
    }

    #[test]
    fn install_record_round_trips() {
        let record = InstallRecord {
            path: "C:\\apps\\slate.exe".into(),
            version: "0.1.1".into(),
            pid: 4321,
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: InstallRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn install_record_tolerates_missing_fields() {
        let back: InstallRecord = serde_json::from_str("{}").unwrap();
        assert_eq!(back, InstallRecord::default());
    }

    #[test]
    fn saved_sets_round_trip() {
        let sets = SavedSets {
            schema_version: SCHEMA_VERSION,
            sets: vec![SavedSet {
                id: 1,
                name: "Work".into(),
                tabs: vec![SavedTab {
                    exe_path: "C:\\x\\chrome.exe".into(),
                    exe_name: "chrome.exe".into(),
                    title_hint: "Tab".into(),
                    accent: None,
                }],
            }],
        };
        let json = serde_json::to_string(&sets).unwrap();
        let back: SavedSets = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sets.len(), 1);
        assert_eq!(back.sets[0].name, "Work");
        assert_eq!(back.sets[0].tabs[0].exe_name, "chrome.exe");
    }
}
