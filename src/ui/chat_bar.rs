use crate::app::state::AppState;
use eframe::egui;

pub fn draw_chat_bar(ui: &mut egui::Ui, state: &mut AppState, send_action: &mut Option<bool>) {
    if let AppState::LoggedIn {
        message_to_send,
        current_channel,
        send_in_progress,
        last_error,
        ..
    } = state
    {
        ui.horizontal(|ui| {
            let response =
                ui.add(egui::TextEdit::singleline(message_to_send).hint_text("Enter message..."));
            let enter_pressed =
                response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let can_send =
                !message_to_send.is_empty() && current_channel.is_some() && !*send_in_progress;
            if ui
                .add_enabled(can_send, egui::Button::new("Send"))
                .clicked()
                || (enter_pressed && can_send)
            {
                *send_action = Some(false);
            }
            if ui
                .add_enabled(can_send, egui::Button::new("Announce"))
                .clicked()
            {
                *send_action = Some(true);
            }
            if *send_in_progress {
                ui.spinner();
            }
        });
        if let Some(error) = last_error {
            ui.colored_label(egui::Color32::RED, error);
        }
    }
}
