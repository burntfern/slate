use std::path::PathBuf;
use std::time::Duration;

use ely_gpui_component::feedback::Toast;
use ely_gpui_component::primitives::Severity;
use gpui::Context;

use super::dto::update_toast;
use super::Controller;
use crate::config;
use crate::core::update::{self, Startup, UpdateState, CHECK_COOLDOWN_SECS};
use crate::win::shell::open_url;
use crate::win::{activate_process, process_alive};

/// Startup gate: one live copy per machine state, and this run owns the
/// registered-exe record from here on. Returns an older leftover elsewhere
/// to offer removing, if the record points at a dead older file.
pub(super) fn startup_install() -> Option<(String, String)> {
    let current_exe = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let record = config::load_install();
    let registered = record
        .as_ref()
        .map(|r| (r.path.as_str(), r.version.as_str(), r.pid));
    let other_alive = registered.is_some_and(|(path, _, pid)| {
        !path.eq_ignore_ascii_case(&current_exe) && process_alive(pid)
    });
    let startup = update::classify_startup(
        registered,
        &current_exe,
        update::CURRENT_VERSION,
        other_alive,
    );
    let stale = match &startup {
        Startup::Continue => None,
        Startup::AlreadyRunning { .. } => {
            activate_process(record.as_ref().map_or(0, |r| r.pid));
            crate::win::shell::info_message(
                "Slate is already running",
                "Switching to the open window.",
            );
            std::process::exit(0);
        }
        Startup::StaleCopy { path, version } => Some((path.clone(), version.clone())),
    };
    // The last run owns the record from here on, whatever happens next.
    config::save_install(&config::InstallRecord {
        path: current_exe,
        version: update::CURRENT_VERSION.to_string(),
        pid: std::process::id(),
    });
    stale
}

impl Controller {
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(self.update, UpdateState::Available(_)) {
            self.apply_update(cx);
            return;
        }
        let now = update::now_secs();
        if now.saturating_sub(self.settings.updates.checked_at) < CHECK_COOLDOWN_SECS {
            self.push_toast(
                update_toast(
                    "Checked a moment ago",
                    Severity::Info,
                    "Slate checked just now - try again shortly.",
                ),
                cx,
            );
            return;
        }
        self.settings.updates.checked_at = now;
        self.save_settings();
        self.update = UpdateState::Checking;
        self.invalidate_info();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { crate::win::update::fetch_latest() })
                .await;
            let _ = this.update(cx, |controller, cx| {
                controller.apply_update_result(result, cx)
            });
        })
        .detach();
    }

    fn apply_update_result(&mut self, result: Result<String, String>, cx: &mut Context<Self>) {
        match result {
            Ok(version) if update::is_newer(update::CURRENT_VERSION, &version) => {
                self.settings.updates.latest = Some(version.clone());
                self.save_settings();
                self.update = UpdateState::Available(version.clone());
                let this = cx.entity();
                let toast = Toast::new(format!("v{version} is out"))
                    .severity(Severity::Success)
                    .action("Update now", move |_, cx| {
                        this.update(cx, |controller, cx| controller.apply_update(cx));
                    })
                    // A snackbar with a decision in it stays until answered.
                    .stay(None);
                self.push_toast(toast, cx);
            }
            Ok(_) => {
                self.settings.updates.latest = None;
                self.save_settings();
                self.update = UpdateState::UpToDate;
                let body = format!("Slate v{} is the latest release.", update::CURRENT_VERSION);
                self.push_toast(update_toast("Up to date", Severity::Success, body), cx);
            }
            Err(err) => {
                self.update = UpdateState::Failed;
                self.push_toast(
                    update_toast("Update check failed", Severity::Danger, err),
                    cx,
                );
            }
        }
        self.invalidate_info();
        cx.notify();
    }

    /// Download the available release, swap it in over this exe after exit,
    /// and relaunch. Entry points: the "Update now" toast action and clicking
    /// the menu item again while an update is pending.
    pub(super) fn apply_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(version) = self.update.clone() else {
            return;
        };
        if self.updating {
            return;
        }
        self.updating = true;
        self.push_toast(
            update_toast("Downloading update", Severity::Info, "About a minute"),
            cx,
        );
        self.invalidate_info();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::win::update::download_release(&version) })
                .await;
            let _ = this.update(cx, |controller, cx| controller.finish_update(result, cx));
        })
        .detach();
    }

    fn finish_update(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        self.updating = false;
        self.invalidate_info();
        let downloaded = match result {
            Ok(path) => path,
            Err(err) => {
                let toast = Toast::new("Update failed")
                    .severity(Severity::Danger)
                    .body(err)
                    .action("Open releases page", move |_, _| {
                        open_url(update::RELEASES_URL)
                    });
                self.push_toast(toast, cx);
                cx.notify();
                return;
            }
        };
        let UpdateState::Available(version) = self.update.clone() else {
            return;
        };
        let target = std::env::current_exe().unwrap_or_default();
        let pid = std::process::id();
        if let Err(err) = crate::win::update::spawn_swap_helper(pid, &target, &downloaded) {
            self.push_toast(update_toast("Update failed", Severity::Danger, err), cx);
            cx.notify();
            return;
        }
        self.push_toast(
            Toast::new(format!("Restarting into v{version}")).severity(Severity::Success),
            cx,
        );
        // Give the toast a beat to paint; the helper waits for our exit.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = this.update(cx, |controller, cx| {
                controller.shutdown();
                cx.quit();
            });
        })
        .detach();
    }

    /// Called once the host window exists so the prompt has somewhere to show.
    pub(super) fn offer_stale_cleanup(&mut self, cx: &mut Context<Self>) {
        let Some((path, version)) = self.stale_copy.take() else {
            return;
        };
        let this = cx.entity();
        let target = path.clone();
        // Upgrading in place instead of deleting: any pin or shortcut that
        // points at the old path keeps working, because the path survives.
        let toast = Toast::new("Old copy found")
            .severity(Severity::Info)
            .body(format!(
                "Slate v{version} is left over from an older install. Bring it up to date?"
            ))
            .action("Update", move |_, cx| {
                this.update(cx, |controller, cx| {
                    let source = std::env::current_exe().unwrap_or_default();
                    match std::fs::copy(&source, &target) {
                        Ok(_) => controller.push_toast(
                            Toast::new("Old copy updated").severity(Severity::Success),
                            cx,
                        ),
                        Err(err) => controller.push_toast(
                            update_toast(
                                "Could not update old copy",
                                Severity::Danger,
                                format!("{target}: {err}"),
                            ),
                            cx,
                        ),
                    }
                });
            })
            .stay(None);
        self.push_toast(toast, cx);
    }

    pub(super) fn push_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toaster.update(cx, |toaster, cx| {
            toaster.push(toast, cx);
        });
    }
}
