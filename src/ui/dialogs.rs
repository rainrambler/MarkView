//! 弹窗：关于、快捷键速查、以及"有未保存改动"的确认框。

use egui::{Context, Modal, RichText, Ui};

use crate::app::{Action, AfterDiscard, App, Dialog};
use crate::i18n::{Strings, fill};

pub fn show(app: &mut App, ctx: &Context, actions: &mut Vec<Action>) {
    match app.dialog.clone() {
        Dialog::None => {}
        Dialog::About => {
            if about(app, ctx) {
                app.dialog = Dialog::None;
            }
        }
        Dialog::Shortcuts => {
            if shortcuts(app, ctx) {
                app.dialog = Dialog::None;
            }
        }
        Dialog::Discard { then } => match discard(app, ctx, &then) {
            Choice::None => {}
            Choice::Cancel => app.dialog = Dialog::None,
            // 先保存，保存成功后再继续原来那件事
            Choice::SaveAndProceed => actions.push(Action::SaveAndProceed(then)),
            Choice::Discard => {
                app.dialog = Dialog::None;
                actions.push(then.force_action());
            }
        },
    }
}

/// `Modal::show` 返回是否应该关闭（点背景、按 Esc、或点了关闭按钮）。
fn about(app: &mut App, ctx: &Context) -> bool {
    let s = app.prefs.language.strings();
    let response = Modal::new(egui::Id::new("markview.dialog.about")).show(ctx, |ui| {
        ui.set_max_width(470.0);
        ui.heading(s.app_name);
        ui.add_space(2.0);
        ui.label(
            RichText::new(fill(
                s.about_version,
                &[("version", env!("CARGO_PKG_VERSION"))],
            ))
            .small()
            .weak(),
        );
        ui.add_space(12.0);
        ui.label(s.about_tagline);

        ui.add_space(14.0);
        ui.label(RichText::new(s.about_stack_title).strong());
        for line in s.about_stack {
            ui.label(RichText::new(format!("· {line}")).small());
        }

        if let Some(path) = app.font_path {
            ui.add_space(12.0);
            ui.label(RichText::new(fill(s.font_note, &[("path", path)])).small().weak());
        }

        ui.add_space(14.0);
        ui.label(RichText::new(s.about_hint).small().weak());
    });

    response.should_close()
}

fn shortcuts(app: &App, ctx: &Context) -> bool {
    let s = app.prefs.language.strings();

    let response = Modal::new(egui::Id::new("markview.dialog.shortcuts")).show(ctx, |ui| {
        ui.set_max_width(430.0);
        ui.heading(s.shortcuts_title);
        ui.add_space(10.0);
        for (key, description) in s.shortcuts {
            if description.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(*key).strong());
                continue;
            }
            row(ui, key, description);
        }
    });

    response.should_close()
}

fn row(ui: &mut Ui, key: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).monospace().small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(description).small());
        });
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    None,
    Cancel,
    SaveAndProceed,
    Discard,
}

fn discard(app: &mut App, ctx: &Context, then: &AfterDiscard) -> Choice {
    let s = app.prefs.language.strings();
    let mut choice = Choice::None;

    let response = Modal::new(egui::Id::new("markview.dialog.discard")).show(ctx, |ui| {
        ui.set_max_width(430.0);
        ui.heading(s.discard_title);
        ui.add_space(8.0);
        ui.label(fill(
            s.discard_body,
            &[("name", &app.doc.display_name(s))],
        ));
        ui.add_space(4.0);
        ui.label(RichText::new(describe(then, s)).small().weak());
        ui.add_space(16.0);

        ui.horizontal(|ui| {
            if ui.button(s.discard_save_continue).clicked() {
                choice = Choice::SaveAndProceed;
            }
            if ui.button(s.discard_discard).clicked() {
                choice = Choice::Discard;
            }
            if ui.button(s.discard_cancel).clicked() {
                choice = Choice::Cancel;
            }
        });
    });

    if choice != Choice::None {
        return choice;
    }
    if response.should_close() {
        return Choice::Cancel;
    }
    Choice::None
}

fn describe(then: &AfterDiscard, s: &Strings) -> &'static str {
    match then {
        AfterDiscard::NewFile => s.discard_new_file,
        AfterDiscard::OpenDialog => s.discard_open_dialog,
        AfterDiscard::OpenPath(_) => s.discard_open_path,
        AfterDiscard::Quit => s.discard_quit,
    }
}
