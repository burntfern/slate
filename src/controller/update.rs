use ely_gpui_component::feedback::Toast;
use ely_gpui_component::primitives::Severity;
use gpui::Context;

use super::dto::update_toast;
use super::Controller;
use crate::core::update::{self, UpdateState, CHECK_COOLDOWN_SECS, RELEASES_URL};
use crate::win::shell::open_url;

impl Controller {
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(self.update, UpdateState::Available(_)) {
            open_url(RELEASES_URL);
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
        let (state, toast) = match result {
            Ok(version) if update::is_newer(update::CURRENT_VERSION, &version) => {
                self.settings.updates.latest = Some(version.clone());
                self.save_settings();
                let body =
                    format!("Slate v{version} is out. Open this menu item again to download it.");
                (
                    UpdateState::Available(version),
                    update_toast("Update available", Severity::Success, body),
                )
            }
            Ok(_) => {
                self.settings.updates.latest = None;
                self.save_settings();
                let body = format!("Slate v{} is the latest release.", update::CURRENT_VERSION);
                (
                    UpdateState::UpToDate,
                    update_toast("Up to date", Severity::Success, body),
                )
            }
            Err(err) => (
                UpdateState::Failed,
                update_toast("Update check failed", Severity::Danger, err),
            ),
        };
        self.update = state;
        self.push_toast(toast, cx);
        self.invalidate_info();
        cx.notify();
    }

    fn push_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toaster.update(cx, |toaster, cx| {
            toaster.push(toast, cx);
        });
    }
}
