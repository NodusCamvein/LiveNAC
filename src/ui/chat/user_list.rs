use crate::app::state::AppState;
use eframe::egui::{self, ScrollArea};

pub fn draw_user_list(ui: &mut egui::Ui, state: &mut AppState) {
    if let AppState::LoggedIn { users, .. } = state {
        ui.heading("Users");
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut sorted_users: Vec<_> = users.iter().collect();
                sorted_users.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                for user in sorted_users {
                    ui.label(&user.name);
                }
            });
    }
}
