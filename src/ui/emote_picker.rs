use crate::app::state::AppState;
use eframe::egui;

pub fn draw_emote_picker(ui: &mut egui::Ui, state: &mut AppState) {
    if let AppState::LoggedIn {
        message_to_send, ..
    } = state
    {
        ui.heading("Emote Picker (Placeholder)");
        ui.label("TODO: Fetch and display emotes");

        // Placeholder emotes
        let placeholder_emotes = vec![":)", ":(", ":D", ";)"];

        ui.horizontal_wrapped(|ui| {
            for emote in placeholder_emotes {
                if ui.button(emote).clicked() {
                    message_to_send.push_str(emote);
                }
            }
        });
    }
}
