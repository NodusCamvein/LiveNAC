use crate::auth::{AuthClient, AuthMessage};
use crate::chat::{AnnouncementColor, ChatClient};
use crate::config;
use crate::eventsub::EventSubClient;
use eframe::egui::{self, Align, FontDefinitions, FontFamily, Key, Layout, ScrollArea, TopBottomPanel, MenuBar};
use std::sync::Arc;
use tokio::{sync::mpsc, task::JoinHandle};
use twitch_oauth2::UserToken;
use twitch_types::UserId;

/// A message sent from a background task to the UI thread.
#[derive(Debug)]
pub enum UiMessage {
    ConfigLoaded(Result<config::Config, eyre::Report>),
    Auth(AuthMessage),
    Chat(ChatUiMessage),
}

/// A message specifically for chat-related UI updates.
#[derive(Debug)]
pub enum ChatUiMessage {
    NewChatMessage(String),
    MessageSent,
    MessageSendError(String),
    EventSubError(String),
}

/// Holds the URI and code for the device flow.
#[derive(Clone, Debug, Default)]
struct DeviceFlowInfo {
    uri: String,
    user_code: String,
}

/// Represents the various states of the application's lifecycle.
enum AppState {
    LoadingConfig,
    FirstTimeSetup {
        client_id_input: String,
        error: Option<String>,
    },
    Authenticating {
        status_message: String,
        device_flow_info: Option<DeviceFlowInfo>,
    },
    LoggedIn {
        token: Arc<UserToken>,
        user_id: UserId,
        user_login: String,
        channel_to_join: String,
        current_channel: Option<String>,
        message_to_send: String,
        chat_messages: Vec<String>,
        chat_client: ChatClient,
        send_in_progress: bool,
        last_error: Option<String>,
        eventsub_task: Option<JoinHandle<()>>,
    },
}

pub struct LiveNAC {
    state: AppState,
    ui_message_rx: mpsc::Receiver<UiMessage>,
    ui_message_tx: mpsc::Sender<UiMessage>,
    config: config::Config,
    show_settings_window: bool,
    show_toolbar: bool,
    last_applied_font_size: f32,
}

impl LiveNAC {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (ui_message_tx, ui_message_rx) = mpsc::channel(100);
        let tx = ui_message_tx.clone();
        tokio::spawn(async move {
            let config_result = config::load().await;
            let _ = tx.send(UiMessage::ConfigLoaded(config_result)).await;
        });

        let default_config = config::Config::default();

        Self {
            state: AppState::LoadingConfig,
            ui_message_rx,
            ui_message_tx,
            config: default_config.clone(),
            show_settings_window: false,
            show_toolbar: false,
            last_applied_font_size: default_config.font_size,
        }
    }
}

impl eframe::App for LiveNAC {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_settings(ctx);

        while let Ok(msg) = self.ui_message_rx.try_recv() {
            self.handle_message(msg);
        }

        let mut send_action: Option<bool> = None;
        let mut login_action: Option<String> = None;

        match &mut self.state {
            AppState::LoadingConfig => self.draw_loading_ui(ctx),
            AppState::FirstTimeSetup { .. } => self.draw_first_time_setup(ctx, &mut login_action),
            AppState::Authenticating { .. } => self.draw_authenticating_ui(ctx),
            AppState::LoggedIn { .. } => self.draw_logged_in(ctx, &mut send_action),
        }

        if let Some(is_announcement) = send_action {
            self.send_message(is_announcement);
        }
        if let Some(client_id) = login_action {
            self.handle_login_action(client_id);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl LiveNAC {
    fn handle_message(&mut self, msg: UiMessage) {
        match msg {
            UiMessage::ConfigLoaded(config_result) => {
                self.handle_config_loaded(config_result)
            }
            UiMessage::Auth(auth_message) => self.handle_auth_message(auth_message),
            UiMessage::Chat(chat_message) => self.handle_chat_message(chat_message),
        }
    }

    fn handle_config_loaded(&mut self, result: Result<config::Config, eyre::Report>) {
        match result {
            Ok(config) => {
                let client_id = config.client_id.clone();
                self.config = config;
                if let Some(id) = client_id {
                    self.start_authentication(id);
                } else {
                    self.state = AppState::FirstTimeSetup {
                        client_id_input: String::new(),
                        error: None,
                    };
                }
            }
            Err(e) => self.state = AppState::FirstTimeSetup {
                client_id_input: String::new(),
                error: Some(format!("Failed to load config: {}", e)),
            },
        }
    }

    fn handle_auth_message(&mut self, msg: AuthMessage) {
        if let AppState::Authenticating { .. } = &mut self.state {
            match msg {
                AuthMessage::AwaitingDeviceActivation { uri, user_code } => {
                    if let AppState::Authenticating { status_message, device_flow_info } = &mut self.state {
                        *status_message = "Please authorize in your browser.".to_string();
                        *device_flow_info = Some(DeviceFlowInfo { uri, user_code });
                    }
                }
                AuthMessage::Success(token) => {
                    let user_id = token.user_id.clone();
                    let user_login = token.login.to_string();
                    self.state = AppState::LoggedIn {
                        token: Arc::new(token),
                        user_id,
                        user_login,
                        channel_to_join: String::new(),
                        current_channel: None,
                        message_to_send: String::new(),
                        chat_messages: Vec::new(),
                        chat_client: ChatClient::new(),
                        send_in_progress: false,
                        last_error: None,
                        eventsub_task: None,
                    };
                }
                AuthMessage::Error(err) => self.state = AppState::FirstTimeSetup {
                    client_id_input: String::new(),
                    error: Some(format!("Authentication Failed: {}", err)),
                },
            }
        }
    }

    fn handle_chat_message(&mut self, msg: ChatUiMessage) {
        if let AppState::LoggedIn {
            chat_messages,
            send_in_progress,
            last_error,
            message_to_send,
            ..
        } = &mut self.state
        {
            match msg {
                ChatUiMessage::NewChatMessage(message) => chat_messages.push(message),
                ChatUiMessage::MessageSent => {
                    *send_in_progress = false;
                    message_to_send.clear();
                }
                ChatUiMessage::MessageSendError(err) => {
                    *send_in_progress = false;
                    *last_error = Some(err);
                }
                ChatUiMessage::EventSubError(err) => {
                    *last_error = Some(format!("Chat connection error: {}", err));
                }
            }
            if chat_messages.len() > 200 {
                chat_messages.remove(0);
            }
        }
    }

    fn handle_login_action(&mut self, client_id: String) {
        self.config.client_id = Some(client_id.clone());
        let config_to_save = self.config.clone();
        tokio::spawn(async move {
            if let Err(e) = config::save(&config_to_save).await {
                tracing::error!("Failed to save config: {}", e);
            }
        });
        self.start_authentication(client_id);
    }

    fn start_authentication(&mut self, client_id: String) {
        self.state = AppState::Authenticating {
            status_message: "Logging in...".to_string(),
            device_flow_info: None,
        };
        let tx = self.ui_message_tx.clone();
        tokio::spawn(async move {
            match AuthClient::new(client_id, tx.clone()) {
                Ok(auth_client) => auth_client.get_or_refresh_token().await,
                Err(e) => {
                    let _ = tx
                        .send(UiMessage::Auth(AuthMessage::Error(format!(
                            "Initialization failed: {}",
                            e
                        ))))
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

    fn draw_first_time_setup(
        &mut self,
        ctx: &egui::Context,
        login_action: &mut Option<String>,
    ) {
        if let AppState::FirstTimeSetup {
            client_id_input,
            error,
        } = &mut self.state
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(ui.available_height() * 0.2);
                    ui.heading("LiveNAC");
                    ui.label("Twitch Chat Client");
                });
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.add_space(ui.available_height() * 0.4);
                    if ui.button("Login").clicked() && !client_id_input.is_empty() {
                        *login_action = Some(client_id_input.clone());
                    }
                    ui.add(
                        egui::TextEdit::singleline(client_id_input)
                            .hint_text("Enter your Twitch Client ID here"),
                    );
                    if let Some(err) = error {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                });
            });
        }
    }

    fn draw_authenticating_ui(&self, ctx: &egui::Context) {
        if let AppState::Authenticating {
            device_flow_info, ..
        } = &self.state
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    if let Some(info) = device_flow_info {
                        ui.with_layout(Layout::top_down(Align::Center), |ui| {
                            ui.heading("Login to Twitch");
                            ui.label("Please go to the following URL in your browser:");
                            ui.hyperlink(&info.uri);
                            ui.add_space(10.0);
                            ui.label("And enter this code:");
                            ui.heading(&info.user_code);
                            ui.add_space(10.0);
                            ui.spinner();
                        });
                    } else {
                        ui.spinner();
                    }
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
            message_to_send,
            send_in_progress,
            last_error,
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
                    MenuBar::new().ui(ui, |ui| {
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
                        let tx = self.ui_message_tx.clone();
                        let token = token.clone();
                        let user_id = user_id.clone();
                        let channel_login = channel_to_join.clone();
                        *eventsub_task = Some(tokio::spawn(async move {
                            let chat_client = ChatClient::new();
                            match chat_client.get_user_id(&channel_login, &token).await {
                                Ok(Some(id)) => {
                                    let eventsub_client =
                                        EventSubClient::new(user_id, token, tx, id);
                                    if let Err(e) = eventsub_client.run().await {
                                        tracing::error!("EventSub client failed: {}", e);
                                    }
                                }
                                _ => {
                                    let _ = tx
                                        .send(UiMessage::Chat(ChatUiMessage::EventSubError(
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
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(message_to_send)
                            .hint_text("Enter message..."),
                    );
                    let enter_pressed =
                        response.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
                    let can_send = !message_to_send.is_empty()
                        && current_channel.is_some()
                        && !*send_in_progress;
                    if ui.add_enabled(can_send, egui::Button::new("Send")).clicked()
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
            });

            egui::CentralPanel::default().show(ctx, |ui| {
                ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for message in chat_messages.iter() {
                            ui.label(message);
                        }
                    });
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
                let tx = self.ui_message_tx.clone();
                let message = message_to_send.clone();
                tokio::spawn(async move {
                    let broadcaster_id = match chat_client.get_user_id(&channel, &token).await {
                        Ok(Some(id)) => id,
                        _ => {
                            let _ = tx
                                .send(UiMessage::Chat(ChatUiMessage::MessageSendError(
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
                        Ok(_) => tx.send(UiMessage::Chat(ChatUiMessage::MessageSent)).await,
                        Err(e) => tx
                            .send(UiMessage::Chat(ChatUiMessage::MessageSendError(
                                format!("Failed to send: {}", e),
                            )))
                            .await,
                    };
                });
            }
        }
    }
}
