use gpui::Context;

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
        if now - self.settings.updates.checked_at < CHECK_COOLDOWN_SECS {
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
        self.update = match result {
            Ok(version) if update::is_newer(update::CURRENT_VERSION, &version) => {
                self.settings.updates.latest = Some(version.clone());
                self.save_settings();
                UpdateState::Available(version)
            }
            Ok(_) => {
                self.settings.updates.latest = None;
                self.save_settings();
                UpdateState::UpToDate
            }
            Err(_) => UpdateState::Failed,
        };
        self.invalidate_info();
        cx.notify();
    }
}
