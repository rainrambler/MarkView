//! 界面层的公共设施与模块声明。

pub mod dialogs;
pub mod editor;
pub mod find;
pub mod menu;
pub mod outline;
pub mod preview;
pub mod status;

use egui::{Context, Id};

use crate::prefs::Preferences;

/// 只在第一次进入时定制一次样式；之后 egui 在深浅色之间切换会带着这些改动。
fn style_tuned() -> Id {
    Id::new("markview.style_tuned")
}

/// 应用主题与样式微调。每帧调用是安全的（`set_theme` 只是写一个选项）。
pub fn install_theme(ctx: &Context, prefs: &Preferences) {
    ctx.set_theme(prefs.theme.to_egui());

    if ctx.memory(|m| m.data.get_temp::<bool>(style_tuned()).unwrap_or(false)) {
        return;
    }
    ctx.memory_mut(|m| m.data.insert_temp(style_tuned(), true));

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(9.0, 3.0);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.visuals.window_corner_radius = egui::CornerRadius::same(8);
        style.visuals.menu_corner_radius = egui::CornerRadius::same(8);
    });
}
