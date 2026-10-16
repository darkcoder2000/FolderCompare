//! App-modal dialogs drawn with `egui::Modal`. The app keeps a stack of [`Dialog`]s; the top one is shown and
//! returns an [`Outcome`] once the user answered.

use egui::{Align, Color32, Key, Layout, RichText, Ui};

use crate::theme;

/// What happens after a confirmation was answered with Yes.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    /// Just a message; nothing follows.
    None,
    ReplaceProfile(usize),
    DeleteProfile(usize),
}

#[derive(Debug)]
pub enum Dialog {
    Message {
        title: String,
        text: String,
        error: bool,
    },
    Confirm {
        title: String,
        text: String,
        warning: bool,
        action: ConfirmAction,
    },
}

#[derive(Debug)]
pub enum Outcome {
    Closed,
    Confirmed(ConfirmAction),
}

impl Dialog {
    pub fn info(title: impl Into<String>, text: impl Into<String>) -> Self {
        Dialog::Message {
            title: title.into(),
            text: text.into(),
            error: false,
        }
    }

    pub fn error(title: impl Into<String>, text: impl Into<String>) -> Self {
        Dialog::Message {
            title: title.into(),
            text: text.into(),
            error: true,
        }
    }

    pub fn confirm(
        title: impl Into<String>,
        text: impl Into<String>,
        action: ConfirmAction,
    ) -> Self {
        Dialog::Confirm {
            title: title.into(),
            text: text.into(),
            warning: false,
            action,
        }
    }

    /// Shows the dialog; returns the outcome when it was answered this frame.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        let mut outcome = None;
        let response = egui::Modal::new(egui::Id::new("fc-dialog")).show(ctx, |ui| {
            ui.set_max_width(560.0);
            match self {
                Dialog::Message { title, text, error } => {
                    title_line(ui, title);
                    let color = if *error { theme::ERROR } else { theme::TEXT };
                    ui.label(RichText::new(text.as_str()).color(color));
                    ui.add_space(8.0);
                    let ok = buttons_right(ui, |ui| ui.button("  OK  ").clicked());
                    if ok || enter(ui) {
                        outcome = Some(Outcome::Closed);
                    }
                }
                Dialog::Confirm {
                    title,
                    text,
                    warning,
                    action,
                } => {
                    title_line(ui, title);
                    let color = if *warning { theme::ERROR } else { theme::TEXT };
                    ui.label(RichText::new(text.as_str()).color(color));
                    ui.add_space(8.0);
                    // Default is No: Enter answers No, like the WPF message box.
                    let (yes, no) = buttons_right(ui, |ui| {
                        let no = ui.button("  No  ");
                        let yes = ui.button("  Yes  ");
                        (yes.clicked(), no.clicked())
                    });
                    if yes {
                        outcome = Some(Outcome::Confirmed(action.clone()));
                    } else if no || enter(ui) {
                        outcome = Some(Outcome::Closed);
                    }
                }
            }
        });
        if outcome.is_none() && response.should_close() {
            outcome = Some(Outcome::Closed);
        }
        outcome
    }
}

pub fn title_line(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).strong().size(15.0));
    ui.add_space(6.0);
}

/// Lays buttons out right-aligned (the closure adds them right to left).
pub fn buttons_right<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.with_layout(Layout::right_to_left(Align::Center), add)
        .inner
}

fn enter(ui: &Ui) -> bool {
    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
}

/// A colored bar with a message (warnings in dialogs, the root error in the main window).
pub fn message_bar(ui: &mut Ui, text: &str, fill: Color32, color: Color32) {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(color));
        });
}
