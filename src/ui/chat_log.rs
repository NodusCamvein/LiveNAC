use crate::app::state::AppState;
use eframe::egui::{self, ScrollArea};

pub fn draw_chat_log(ui: &mut egui::Ui, state: &mut AppState) {
    if let AppState::LoggedIn { chat_messages, .. } = state {
        ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for message in chat_messages.iter() {
                    ui.label(message);
                }
            });
    }
}
