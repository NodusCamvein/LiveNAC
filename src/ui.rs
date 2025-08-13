use crate::auth::{AuthClient, AuthMessage};
use crate::chat::{AnnouncementColor, ChatClient};
use crate::eventsub::EventSubClient;
use eframe::egui::{self, ScrollArea};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use twitch_oauth2::UserToken;
use twitch_types::UserId;

/// A message related to chat, eventsub, or sending messages.
#[derive(Debug)]
pub enum ChatUiMessage {
    NewChatMessage(String),
    MessageSent,
    MessageSendError(String),
    EventSubError(String),
}

/// Holds the URI and code for the device flow, used to update the UI.
#[derive(Clone, Debug, Default)]
struct DeviceFlowInfo {
    uri: String,
    user_code: String,
}

/// Represents the two main states of the application.
enum AppState {
    LoggedOut {
        client_id_input: String,
        status_message: String,
        device_flow_info: Option<DeviceFlowInfo>,
        login_started: bool,
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
    },
}

/// The main application struct.
pub struct LiveNAC {
    state: AppState,
    tokio_runtime_handle: Handle,
    auth_message_rx: mpsc::Receiver<AuthMessage>,
    auth_message_tx: mpsc::Sender<AuthMessage>,
    chat_ui_message_rx: mpsc::Receiver<ChatUiMessage>,
    chat_ui_message_tx: mpsc::Sender<ChatUiMessage>,
}

impl LiveNAC {
    pub fn new(cc: &eframe::CreationContext<'_>, tokio_runtime_handle: Handle) -> Self {
        cc.egui_ctx.set_pixels_per_point(1.5);
        let (auth_message_tx, auth_message_rx) = mpsc::channel(10);
        let (chat_ui_message_tx, chat_ui_message_rx) = mpsc::channel(100);

        Self {
            state: AppState::LoggedOut {
                client_id_input: String::new(),
                status_message: "Enter your Twitch App Client ID.".to_string(),
                device_flow_info: None,
                login_started: false,
            },
            tokio_runtime_handle,
            auth_message_rx,
            auth_message_tx,
            chat_ui_message_rx,
            chat_ui_message_tx,
        }
    }
}

impl eframe::App for LiveNAC {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Handle Authentication Messages ---
        if let Ok(msg) = self.auth_message_rx.try_recv() {
            match msg {
                AuthMessage::AwaitingDeviceActivation { uri, user_code } => {
                    if let AppState::LoggedOut {
                        status_message,
                        device_flow_info,
                        ..
                    } = &mut self.state
                    {
                        *status_message = "Go to the URL and enter the code.".to_string();
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
                    };
                }
                AuthMessage::Error(err) => {
                    if let AppState::LoggedOut {
                        status_message,
                        login_started,
                        device_flow_info,
                        ..
                    } = &mut self.state
                    {
                        *status_message = format!("Error: {}", err);
                        *login_started = false;
                        *device_flow_info = None;
                    }
                }
            }
        }

        // --- Handle Chat UI Messages ---
        if let AppState::LoggedIn {
            chat_messages,
            send_in_progress,
            last_error,
            message_to_send,
            ..
        } = &mut self.state
        {
            while let Ok(msg) = self.chat_ui_message_rx.try_recv() {
                match msg {
                    ChatUiMessage::NewChatMessage(message) => {
                        chat_messages.push(message);
                        if chat_messages.len() > 100 {
                            chat_messages.remove(0);
                        }
                    }
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
            }
        }

        // --- Draw the UI ---
        // This is deferred until after the UI is drawn to avoid borrow checker errors.
        let mut send_action: Option<bool> = None;

        match &mut self.state {
            AppState::LoggedOut { .. } => self.draw_logged_out(ctx),
            AppState::LoggedIn { .. } => self.draw_logged_in(ctx, &mut send_action),
        }

        if let Some(is_announcement) = send_action {
            self.send_message(is_announcement);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl LiveNAC {
    fn draw_logged_out(&mut self, ctx: &egui::Context) {
        if let AppState::LoggedOut {
            client_id_input,
            status_message,
            device_flow_info,
            login_started,
        } = &mut self.state
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("LiveNAC: Twitch Chat Client");
                ui.add_space(20.0);
                ui.label(status_message.as_str());
                ui.add_space(10.0);

                if let Some(info) = &device_flow_info {
                    ui.heading("Login to Twitch");
                    ui.label("Please go to the following URL in your browser:");
                    ui.hyperlink(&info.uri);
                    ui.add_space(10.0);
                    ui.label("And enter this code:");
                    ui.heading(&info.user_code);
                    ui.add_space(10.0);
                    ui.spinner();
                } else {
                    ui.label("Twitch Application Client ID:");
                    ui.text_edit_singleline(client_id_input);

                    let is_ready_to_login =
                        !client_id_input.trim().is_empty() && !*login_started;

                    ui.add_enabled_ui(is_ready_to_login, |ui| {
                        if ui.button("Login to Twitch").clicked() {
                            *login_started = true;
                            *status_message = "Attempting to log in...".to_string();

                            let client_id = client_id_input.trim().to_string();
                            let auth_tx = self.auth_message_tx.clone();
                            let runtime_handle = self.tokio_runtime_handle.clone();

                            self.tokio_runtime_handle.spawn(async move {
                                match AuthClient::new(client_id, auth_tx.clone()) {
                                    Ok(auth_client) => {
                                        runtime_handle.spawn(async move {
                                            auth_client.get_or_refresh_token().await;
                                        });
                                    }
                                    Err(e) => {
                                        let _ = auth_tx
                                            .send(AuthMessage::Error(format!(
                                                "Initialization failed: {}",
                                                e
                                            )))
                                            .await;
                                    }
                                }
                            });
                        }
                    });
                }
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
            chat_client,
        } = &mut self.state
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading(format!("Logged in as {}", user_login));
                ui.add_space(10.0);

                // Channel joining section
                ui.horizontal(|ui| {
                    ui.label("Channel:");
                    ui.text_edit_singleline(channel_to_join);
                    if ui.button("Join").clicked() && !channel_to_join.is_empty() {
                        *current_channel = Some(channel_to_join.clone());
                        let chat_ui_tx = self.chat_ui_message_tx.clone();
                        let token = token.clone();
                        let channel_login = channel_to_join.clone();
                        let user_id = user_id.clone();
                        let chat_client = chat_client.clone();

                        self.tokio_runtime_handle.spawn(async move {
                            match chat_client.get_user_id(&channel_login, &token).await {
                                Ok(Some(broadcaster_id)) => {
                                    let eventsub_client =
                                        EventSubClient::new(user_id, token, chat_ui_tx);
                                    if let Err(e) = eventsub_client.run(broadcaster_id).await {
                                        tracing::error!("EventSub client failed: {}", e);
                                    }
                                }
                                _ => {
                                    let _ = chat_ui_tx
                                        .send(ChatUiMessage::EventSubError(
                                            "Channel not found".to_string(),
                                        ))
                                        .await;
                                }
                            }
                        });
                    }
                });

                ui.label(format!(
                    "Current Channel: {}",
                    current_channel.as_deref().unwrap_or("None")
                ));

                // Chat messages area
                ui.add_space(10.0);
                ui.separator();
                ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for message in chat_messages.iter() {
                            ui.label(message);
                        }
                    });

                // Message input section
                ui.separator();
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(message_to_send);

                    let can_send = !message_to_send.is_empty()
                        && current_channel.is_some()
                        && !*send_in_progress;

                    let send_button = ui.add_enabled(can_send, egui::Button::new("Send"));
                    if send_button.clicked() {
                        *send_action = Some(false);
                    }

                    let send_announcement_button =
                        ui.add_enabled(can_send, egui::Button::new("Send Announcement"));
                    if send_announcement_button.clicked() {
                        *send_action = Some(true);
                    }

                    if *send_in_progress {
                        ui.spinner();
                    }
                });

                if let Some(error) = last_error {
                    ui.label(egui::RichText::new(error.as_str()).color(egui::Color32::RED));
                }
            });
        }
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
                let ui_tx = self.chat_ui_message_tx.clone();
                let message = message_to_send.clone();

                self.tokio_runtime_handle.spawn(async move {
                    let broadcaster_id = match chat_client.get_user_id(&channel, &token).await {
                        Ok(Some(id)) => id,
                        _ => {
                            let _ = ui_tx
                                .send(ChatUiMessage::MessageSendError(
                                    "Channel not found".to_string(),
                                ))
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

                    match result {
                        Ok(_) => {
                            let _ = ui_tx.send(ChatUiMessage::MessageSent).await;
                        }
                        Err(e) => {
                            let _ = ui_tx
                                .send(ChatUiMessage::MessageSendError(format!(
                                    "Failed to send: {}",
                                    e
                                )))
                                .await;
                        }
                    }
                });
            }
        }
    }
}
