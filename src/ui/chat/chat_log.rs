use crate::{
    app::{config::Config, state::AppState},
    models::message::MessageFragment,
    utils::text_processing::{parse_text_for_urls, TextOrUrl},
};
use eframe::egui::{self, Color32, Image, RichText, ScrollArea, Vec2};

pub fn draw_chat_log(ui: &mut egui::Ui, state: &mut AppState, config: &Config) {
    if let AppState::LoggedIn { chat_messages, .. } = state {
        ScrollArea::vertical()
            .id_salt("chat_log_scroll_area")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for message in chat_messages.iter() {
                    ui.horizontal_wrapped(|ui| {
                        ui.set_min_height(config.emote_size);

                        if config.show_timestamps {
                            let timestamp_str = message.timestamp.format("[%H:%M:%S]").to_string();
                            ui.label(RichText::new(timestamp_str).color(Color32::from_gray(128)));
                        }

                        if config.collapse_emotes {
                            // This is a bit of a hack, but it's the easiest way to
                            // remove the space between emotes.
                            ui.spacing_mut().item_spacing.x = 0.0;
                        }

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
                                    for segment in parse_text_for_urls(text) {
                                        match segment {
                                            TextOrUrl::Text(t) => {
                                                ui.label(RichText::new(t));
                                            }
                                            TextOrUrl::Url(u) => {
                                                ui.hyperlink(&u);
                                            }
                                        }
                                    }
                                }
                                MessageFragment::Emote(emote) => {
                                    let image = Image::new(emote.url.as_str())
                                        .max_size(Vec2::new(config.emote_size, config.emote_size));

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
