use ely_gpui_component::feedback::Toast;
use ely_gpui_component::primitives::Severity;
use gpui::Context;

use super::dto::update_toast;
use super::Controller;
use crate::config;
use crate::core::update::{self, UpdateState, CHECK_COOLDOWN_SECS};
use crate::win::process_alive;
use crate::win::shell::open_url;

pub(super) fn startup_install() -> crate::core::install::Plan {
    let current_exe = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let record = config::load_install();
    let registered = record.as_ref().map(|r| crate::core::install::Record {
        path: r.path.as_str(),
        version: r.version.as_str(),
        pid: r.pid,
    });
    let record_canonical_alive = registered.is_some_and(|r| {
        r.pid != std::process::id()
            && r.path
                .eq_ignore_ascii_case(config::canonical_exe_path().to_string_lossy().as_ref())
            && process_alive(r.pid)
    });
    let canonical_exists = config::canonical_exe_path().exists();
    let plan = crate::core::install::classify(crate::core::install::Situation {
        exe: current_exe.as_str(),
        canonical: config::canonical_exe_path().to_string_lossy().as_ref(),
        canonical_exists,
        record: registered,
        record_canonical_alive,
    });
    plan
}

impl Controller {
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(self.update, UpdateState::Available(_)) {
            open_url(update::RELEASES_URL);
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
                let toast = Toast::new(format!("v{version} is out"))
                    .severity(Severity::Success)
                    .body("Download it from the releases page.")
                    .action("Open releases page", move |_, _| {
                        open_url(update::RELEASES_URL)
                    });
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

    pub(super) fn push_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toaster.update(cx, |toaster, cx| {
            toaster.push(toast, cx);
        });
    }
}
