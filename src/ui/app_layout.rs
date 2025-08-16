use crate::{
    app::{
        config::{self, Config},
        reducer,
        state::AppState,
    },
    core::{
        auth::AuthClient,
        chat::{AnnouncementColor, ChatClient},
        eventsub::EventSubClient,
    },
    events::app_event::{AppEvent, ChatEvent},
    ui::{chat_bar, chat_log, emote_picker, user_list},
};
use eframe::egui::{
    self, Align, FontDefinitions, FontFamily, Key, Layout, SidePanel, TopBottomPanel,
};
use tokio::sync::mpsc;

pub struct App {
    state: AppState,
    event_rx: mpsc::Receiver<AppEvent>,
    event_tx: mpsc::Sender<AppEvent>,
    config: Config,
    show_settings_window: bool,
    show_toolbar: bool,
    show_emote_picker: bool,
    last_applied_font_size: f32,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let config_result = config::load().await;
            let _ = tx.send(AppEvent::ConfigLoaded(config_result)).await;
        });

        let default_config = Config::default();

        Self {
            state: AppState::LoadingConfig,
            event_rx,
            event_tx,
            config: default_config.clone(),
            show_settings_window: false,
            show_toolbar: false,
            show_emote_picker: false,
            last_applied_font_size: default_config.font_size,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_settings(ctx);

        while let Ok(event) = self.event_rx.try_recv() {
            reducer::reduce(&mut self.state, event);
        }

        let mut send_action: Option<bool> = None;
        let mut login_action: Option<bool> = None;

        match &mut self.state {
            AppState::LoadingConfig => self.draw_loading_ui(ctx),
            AppState::FirstTimeSetup { .. } => self.draw_first_time_setup(ctx, &mut login_action),
            AppState::Authenticating { .. } => self.draw_authenticating_ui(ctx),
            AppState::LoggedIn { .. } => self.draw_logged_in(ctx, &mut send_action),
        }

        if let Some(is_announcement) = send_action {
            self.send_message(is_announcement);
        }
        if let Some(true) = login_action {
            self.handle_login_action();
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl App {
    fn handle_login_action(&mut self) {
        if let Some(client_id) = self.config.client_id.clone() {
            self.start_authentication(client_id);
        } else {
            tracing::error!("Client ID not found in config!");
            if let AppState::FirstTimeSetup { error, .. } = &mut self.state {
                *error = Some("Client ID not found in configuration. Please check your config file.".to_string());
            }
        }
    }

    fn start_authentication(&mut self, client_id: String) {
        self.state = AppState::Authenticating {
            status_message: "Logging in...".to_string(),
        };
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            match AuthClient::new(client_id, tx.clone()) {
                Ok(auth_client) => auth_client.authenticate().await,
                Err(e) => {
                    let _ = tx
                        .send(AppEvent::Auth(crate::core::auth::AuthMessage::Error(
                            format!("Initialization failed: {}", e),
                        )))
                        .await;
                }
            }
        });
    }

    fn apply_settings(&mut self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        let mut fonts = FontDefinitions::default();

        if self.config.enable_cjk_font {
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "Noto Sans CJK JP".to_owned());
        }

        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size = self.config.font_size;
        });

        ctx.set_style(style);

        if self.last_applied_font_size != self.config.font_size {
            ctx.set_fonts(fonts);
            self.last_applied_font_size = self.config.font_size;
        }
    }

    fn draw_loading_ui(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| ui.spinner());
        });
    }

    fn draw_first_time_setup(&mut self, ctx: &egui::Context, login_action: &mut Option<bool>) {
        if let AppState::FirstTimeSetup { error, .. } = &mut self.state {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(ui.available_height() * 0.2);
                    ui.heading("LiveNAC");
                    ui.label("Twitch Chat Client");
                });
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.add_space(ui.available_height() * 0.4);
                    if ui.button("Login with Twitch").clicked() {
                        *login_action = Some(true);
                    }
                    if let Some(err) = error {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                });
            });
        }
    }

    fn draw_authenticating_ui(&self, ctx: &egui::Context) {
        if let AppState::Authenticating { status_message, .. } = &self.state {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        ui.heading("Authenticating...");
                        ui.label(status_message);
                        ui.add_space(10.0);
                        ui.label("Please check your web browser to continue.");
                        ui.add_space(10.0);
                        ui.spinner();
                    });
                });
            });
        }
    }

    fn draw_logged_in(&mut self, ctx: &egui::Context, send_action: &mut Option<bool>) {
        if let AppState::LoggedIn {
            user_login,
            channel_to_join,
            current_channel,
            chat_messages,

            token,
            user_id,
            eventsub_task,
            ..
        } = &mut self.state
        {
            TopBottomPanel::top("top_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("☰").clicked() {
                        self.show_toolbar = !self.show_toolbar;
                    }
                    ui.heading(format!("Logged in as {}", user_login));
                });

                if self.show_toolbar {
                    egui::MenuBar::new().ui(ui, |ui| {
                        ui.menu_button("File", |ui| {
                            if ui.button("Settings").clicked() {
                                self.show_settings_window = true;
                                ui.close();
                            }
                            if ui.button("Exit").clicked() {
                                std::process::exit(0);
                            }
                        });
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Channel:");
                    let response = ui.text_edit_singleline(channel_to_join);
                    let join_clicked = ui.button("Join").clicked();
                    let enter_pressed =
                        response.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
                    if (join_clicked || enter_pressed) && !channel_to_join.is_empty() {
                        if let Some(task) = eventsub_task.take() {
                            task.abort();
                        }
                        chat_messages.clear();
                        *current_channel = Some(channel_to_join.clone());
                        let tx = self.event_tx.clone();
                        let token = token.clone();
                        let user_id = user_id.clone();
                        let channel_login = channel_to_join.clone();
                        *eventsub_task = Some(tokio::spawn(async move {
                            let chat_client = ChatClient::new();
                            match chat_client.get_user_id(&channel_login, &token).await {
                                Ok(Some(id)) => {
                                    let eventsub_client =
                                        EventSubClient::new(user_id.clone(), token.clone(), tx, id);
                                    if let Err(e) = eventsub_client.run().await {
                                        tracing::error!("EventSub client failed: {}", e);
                                    }
                                }
                                _ => {
                                    let _ = tx
                                        .send(AppEvent::Chat(ChatEvent::EventSubError(
                                            "Channel not found".to_string(),
                                        )))
                                        .await;
                                }
                            }
                        }));
                    }
                });
                ui.label(format!(
                    "Current Channel: {}",
                    current_channel.as_deref().unwrap_or("None")
                ));
            });

            TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
                if self.show_emote_picker {
                    emote_picker::draw_emote_picker(ui, &mut self.state);
                    ui.separator();
                }
                chat_bar::draw_chat_bar(ui, &mut self.state, send_action, &mut self.show_emote_picker);
            });

            SidePanel::right("user_list_panel")
                .min_width(150.0)
                .default_width(180.0)
                .show(ctx, |ui| {
                    user_list::draw_user_list(ui, &mut self.state);
                });

            egui::CentralPanel::default().show(ctx, |ui| {
                chat_log::draw_chat_log(ui, &mut self.state);
            });

            self.draw_settings_window(ctx);
        }
    }

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings")
            .open(&mut self.show_settings_window)
            .show(ctx, |ui| {
                ui.heading("Appearance");
                let mut config_changed = false;
                config_changed |= ui
                    .checkbox(&mut self.config.enable_cjk_font, "Enable CJK Font Support")
                    .changed();
                if self.config.enable_cjk_font {
                    ui.label("Note: Requires a font like 'Noto Sans CJK JP' to be installed on your system.");
                    ui.hyperlink_to("Download Noto Sans CJK from Google Fonts", "https://fonts.google.com/noto/specimen/Noto+Sans+JP");
                }

                config_changed |= ui
                    .add(egui::Slider::new(&mut self.config.font_size, 8.0..=24.0).text("Font Size"))
                    .changed();

                if config_changed {
                    let config_to_save = self.config.clone();
                    tokio::spawn(async move {
                        if let Err(e) = config::save(&config_to_save).await {
                            tracing::error!("Failed to save config: {}", e);
                        }
                    });
                }
            });
    }

    fn send_message(&mut self, is_announcement: bool) {
        if let AppState::LoggedIn {
            current_channel,
            send_in_progress,
            last_error,
            token,
            user_id,
            chat_client,
            message_to_send,
            ..
        } = &mut self.state
        {
            if let Some(channel) = current_channel.clone() {
                *send_in_progress = true;
                *last_error = None;
                let token = token.clone();
                let user_id = user_id.clone();
                let chat_client = chat_client.clone();
                let tx = self.event_tx.clone();
                let message = message_to_send.clone();
                tokio::spawn(async move {
                    let broadcaster_id = match chat_client.get_user_id(&channel, &token).await {
                        Ok(Some(id)) => id,
                        _ => {
                            let _ = tx
                                .send(AppEvent::Chat(ChatEvent::MessageSendError(
                                    "Channel not found".to_string(),
                                )))
                                .await;
                            return;
                        }
                    };
                    let result = if is_announcement {
                        chat_client
                            .send_announcement(
                                broadcaster_id.as_ref(),
                                user_id.as_ref(),
                                &message,
                                Some(AnnouncementColor::Primary),
                                &token,
                            )
                            .await
                    } else {
                        chat_client
                            .send_chat_message(
                                broadcaster_id.as_ref(),
                                user_id.as_ref(),
                                &message,
                                &token,
                            )
                            .await
                    };
                    let _ = match result {
                        Ok(_) => tx.send(AppEvent::Chat(ChatEvent::MessageSent)).await,
                        Err(e) => {
                            tx.send(AppEvent::Chat(ChatEvent::MessageSendError(format!(
                                "Failed to send: {}",
                                e
                            ))))
                            .await
                        }
                    };
                });
            }
        }
    }
}