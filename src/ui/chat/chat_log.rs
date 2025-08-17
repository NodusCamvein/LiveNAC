use crate::{
    app::{config::Config, state::AppState},
    models::message::MessageFragment,
    utils::text_processing::{TextOrUrl, parse_text_for_urls},
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
                            let timestamp_str = message.timestamp.format("[%H:%M:%S] ").to_string();
                            ui.label(RichText::new(timestamp_str).color(Color32::from_gray(128)));
                        }

                        let color = if let Some((r, g, b)) = message.sender_color {
                            Color32::from_rgb(r, g, b)
                        } else {
                            // Default color if none provided
                            Color32::from_gray(160)
                        };
                        let sender =
                            RichText::new(format!("{}: ", message.sender_name)).color(color);
                        ui.label(sender);

                        let original_spacing_x = ui.spacing().item_spacing.x;
                        for (i, fragment) in message.fragments.iter().enumerate() {
                            let is_emote = matches!(fragment, MessageFragment::Emote(_));
                            let mut reset_spacing = true;

                            if i > 0 && config.collapse_emotes {
                                let prev_is_emote = matches!(
                                    message.fragments.get(i - 1),
                                    Some(MessageFragment::Emote(_))
                                );
                                if prev_is_emote && is_emote {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    reset_spacing = false;
                                }
                            }

                            if reset_spacing {
                                ui.spacing_mut().item_spacing.x = original_spacing_x;
                            }

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

                                    let _response = ui
                                        .add(image.sense(egui::Sense::click()))
                                        .on_hover_text(format!("{} - {}", emote.name, source_text));
                                }
                            }
                        }
                        // Restore the original spacing for the next message
                        ui.spacing_mut().item_spacing.x = original_spacing_x;
                    });
                }
            });
    }
}
