use crate::app::state::AppState;
use eframe::egui::{self, ScrollArea};

pub fn draw_user_list(ui: &mut egui::Ui, state: &mut AppState) {
    if let AppState::LoggedIn { users, .. } = state {
        ui.heading("Users");
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for user in users.iter() {
                    ui.label(&user.name);
                }
            });
    }
}
