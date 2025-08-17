use crate::app::config::Config;
use crate::app::state::AppState;
use eframe::egui::{self, Image, ScrollArea, Vec2};

pub fn draw_emote_picker(ui: &mut egui::Ui, state: &mut AppState, config: &Config) {
    if let AppState::LoggedIn {
        global_emotes,
        message_to_send,
        ..
    } = state
    {
        ui.heading("Emotes");

        ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for emote in global_emotes.iter() {
                    let size = Vec2::new(config.emote_size, config.emote_size);
                    let image = Image::new(emote.images.url_1x.as_str()).max_size(size);

                    let response = ui
                        .add(image.sense(egui::Sense::click()))
                        .on_hover_text(format!("{} - {}", emote.name, "Twitch"));

                    if response.clicked() {
                        message_to_send.push_str(&emote.name);
                        message_to_send.push(' ');
                    }
                }
            });
        });
    }
}
