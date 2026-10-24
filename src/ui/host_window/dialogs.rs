use ely_gpui_component::forms::TextInput;
use ely_gpui_component::overlays::PromptDialog;
use gpui::{AnyElement, AppContext, Context, IntoElement, SharedString, Window};

use super::HostWindow;
use crate::controller::{Cmd, HostInfo, SavedPrompt};
use crate::core::GroupId;

impl HostWindow {
    pub(super) fn rename_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        info: &HostInfo,
        group: Option<GroupId>,
    ) -> Option<AnyElement> {
        let (tab_id, group) = match (info.renaming, group) {
            (Some(tab_id), Some(group)) => (tab_id, group),
            _ => {
                self.rename_primed = None;
                return None;
            }
        };
        let field = self
            .rename_field
            .get_or_insert_with(|| cx.new(|cx| TextInput::new(window, cx)))
            .clone();
        if self.rename_primed != Some(tab_id) {
            self.rename_primed = Some(tab_id);
            let current = info
                .tabs
                .iter()
                .find(|t| t.id == tab_id)
                .map(|t| t.title.clone())
                .unwrap_or_default();
            let end = current.len();
            field.update(cx, |f, cx| {
                f.set_text(current, cx);
                f.select(0..end, cx);
            });
        }
        Some(
            PromptDialog::new(("host-rename", 0usize), "Rename tab", &field, {
                let ctrl = self.controller.clone();
                move |_w, cx| {
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::CancelRename, cx));
                }
            })
            .label("Tab name")
            .submit("Rename")
            .check(|text: &str| {
                if text.trim().is_empty() {
                    Err(SharedString::from("Name cannot be empty"))
                } else {
                    Ok(())
                }
            })
            .on_submit({
                let ctrl = self.controller.clone();
                move |text, _w, cx| {
                    let title = text.to_string();
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(
                            Cmd::RenameTab {
                                group,
                                tab: tab_id,
                                title,
                            },
                            cx,
                        );
                        c.dispatch(Cmd::CancelRename, cx);
                    });
                }
            })
            .into_any_element(),
        )
    }

    pub(super) fn saved_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        info: &HostInfo,
    ) -> Option<AnyElement> {
        let prompt = match info.saved_prompt {
            Some(prompt) => prompt,
            None => {
                self.saved_primed = None;
                return None;
            }
        };
        let field = self
            .saved_field
            .get_or_insert_with(|| cx.new(|cx| TextInput::new(window, cx)))
            .clone();
        if self.saved_primed != Some(prompt) {
            self.saved_primed = Some(prompt);
            let current = match prompt {
                SavedPrompt::Save => String::new(),
                SavedPrompt::Rename(id) => info
                    .saved_sets
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
            };
            let end = current.len();
            field.update(cx, |f, cx| {
                f.set_text(current.clone(), cx);
                if !current.is_empty() {
                    f.select(0..end, cx);
                }
            });
        }
        let (title, label, submit) = match prompt {
            SavedPrompt::Save => ("Save or update set", "Set name", "Save / Update"),
            SavedPrompt::Rename(_) => ("Rename tab set", "Set name", "Rename"),
        };
        Some(
            PromptDialog::new(("host-saved-prompt", 0usize), title, &field, {
                let ctrl = self.controller.clone();
                move |_w, cx| {
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::CancelSavedPrompt, cx));
                }
            })
            .label(label)
            .submit(submit)
            .check(|text: &str| {
                if text.trim().is_empty() {
                    Err(SharedString::from("Name cannot be empty"))
                } else {
                    Ok(())
                }
            })
            .on_submit({
                let ctrl = self.controller.clone();
                move |text, _w, cx| {
                    let name = text.to_string();
                    ctrl.update(cx, |c, cx| {
                        match prompt {
                            SavedPrompt::Save => c.dispatch(Cmd::CommitSaveSet { name }, cx),
                            SavedPrompt::Rename(id) => {
                                c.dispatch(Cmd::CommitRenameSet { id, name }, cx)
                            }
                        }
                        c.dispatch(Cmd::CancelSavedPrompt, cx);
                    });
                }
            })
            .into_any_element(),
        )
    }
}
