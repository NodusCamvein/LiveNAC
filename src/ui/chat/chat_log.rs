use crate::{app::state::AppState, models::message::MessageFragment};
use eframe::egui::{self, Color32, Image, RichText, ScrollArea, Vec2};

pub fn draw_chat_log(ui: &mut egui::Ui, state: &mut AppState, emote_size: f32) {
    if let AppState::LoggedIn { chat_messages, .. } = state {
        ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for message in chat_messages.iter() {
                    ui.horizontal_wrapped(|ui| {
                        let color = if let Some((r, g, b)) = message.sender_color {
                            Color32::from_rgb(r, g, b)
                        } else {
                            // Default color if none provided
                            Color32::from_gray(160)
                        };
                        let sender =
                            RichText::new(format!("{}:", message.sender_name)).color(color);
                        ui.label(sender);

                        for fragment in &message.fragments {
                            match fragment {
                                MessageFragment::Text(text) => {
                                    ui.label(text);
                                }
                                MessageFragment::Emote(emote) => {
                                    let image = Image::new(emote.url.as_str())
                                        .max_size(Vec2::new(emote_size, emote_size));

                                    let source_text = format!("{:?}", emote.source);

                                    let _response = ui.add(image.sense(egui::Sense::click()))
                                        .on_hover_text(format!(
                                            "{} - {}",
                                            emote.name, source_text
                                        ));
                                }
                            }
                        }
                    });
                }
            });
    }
}
