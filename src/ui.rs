use crate::eventsub::{EventSubClient, EventSubMessage};
use crate::chat::{ChatClient, AnnouncementColor};
use eframe::egui::{self, ScrollArea};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use twitch_oauth2::{AccessToken, UserToken};
use twitch_types::UserId;

// An enum to handle communication from async tasks to the UI thread.
enum UiMessage {
    LoggedIn {
        token: Arc<UserToken>,
        user_id: UserId,
        user_login: String,
    },
    Error(String),
    MessageSent,
    MessageSendError(String),
}

enum AppState {
    LoggedOut {
        client_id_input: String,
        auth_in_progress: bool,
        error: Option<String>,
    },
    LoggedIn {
        token: Arc<UserToken>,
        user_id: UserId,
        user_login: String,
        channel_to_join: String,
        current_channel: Option<String>,
        message_to_send: String,
        chat_messages: Vec<String>,
        message_rx: mpsc::Receiver<EventSubMessage>,
        message_tx: mpsc::Sender<EventSubMessage>,
        chat_client: ChatClient,
        send_in_progress: bool,
        last_error: Option<String>,
    },
}

pub struct LiveNAC {
    state: AppState,
    tokio_runtime_handle: Handle,
    ui_message_rx: mpsc::Receiver<UiMessage>,
    ui_message_tx: mpsc::Sender<UiMessage>,
}

impl LiveNAC {
    pub fn new(cc: &eframe::CreationContext<'_>, tokio_runtime_handle: Handle) -> Self {
        cc.egui_ctx.set_pixels_per_point(1.5);
        let (ui_message_tx, ui_message_rx) = mpsc::channel(10);
        Self {
            state: AppState::LoggedOut {
                client_id_input: "".to_owned(),
                auth_in_progress: false,
                error: None,
            },
            tokio_runtime_handle,
            ui_message_rx,
            ui_message_tx,
        }
    }
}

impl eframe::App for LiveNAC {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle UI messages
        while let Ok(msg) = self.ui_message_rx.try_recv() {
            match msg {
                UiMessage::LoggedIn {
                    token,
                    user_id,
                    user_login,
                } => {
                    let (message_tx, message_rx) = mpsc::channel(100);
                    self.state = AppState::LoggedIn {
                        token,
                        user_id,
                        user_login,
                        channel_to_join: "".to_owned(),
                        current_channel: None,
                        message_to_send: "".to_owned(),
                        chat_messages: Vec::new(),
                        message_rx,
                        message_tx,
                        chat_client: ChatClient::new(),
                        send_in_progress: false,
                        last_error: None,
                    };
                }
                UiMessage::Error(err) => {
                    if let AppState::LoggedOut {
                        auth_in_progress,
                        error,
                        ..
                    } = &mut self.state
                    {
                        *auth_in_progress = false;
                        *error = Some(err);
                    }
                }
                UiMessage::MessageSent => {
                    if let AppState::LoggedIn { send_in_progress, message_to_send, .. } = &mut self.state {
                        *send_in_progress = false;
                        message_to_send.clear();
                    }
                }
                UiMessage::MessageSendError(err) => {
                    if let AppState::LoggedIn { send_in_progress, last_error, .. } = &mut self.state {
                        *send_in_progress = false;
                        *last_error = Some(err);
                    }
                }
            }
        }

        match &mut self.state {
            AppState::LoggedOut { .. } => {
                self.draw_logged_out(ctx);
            }
            AppState::LoggedIn { message_rx, chat_messages, .. } => {
                // Process incoming chat messages
                while let Ok(message) = message_rx.try_recv() {
                    chat_messages.push(message);
                    // Keep only the last 100 messages to prevent memory issues
                    if chat_messages.len() > 100 {
                        chat_messages.remove(0);
                    }
                }
                self.draw_logged_in(ctx);
            }
        }
        ctx.request_repaint();
    }
}

impl LiveNAC {
    fn draw_logged_out(&mut self, ctx: &egui::Context) {
        if let AppState::LoggedOut {
            client_id_input,
            auth_in_progress,
            error,
        } = &mut self.state
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("LiveNAC: Twitch Chat Client");
                ui.add_space(20.0);
                ui.label("Client ID:");
                ui.text_edit_singleline(client_id_input);

                if *auth_in_progress {
                    ui.spinner();
                    ui.label("Waiting for login...");
                } else if ui.button("Login to Twitch (Simulated)").clicked() {
                    *auth_in_progress = true;
                    let tx = self.ui_message_tx.clone();
                    self.tokio_runtime_handle.spawn(async move {
                        // Simulate login delay
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        
                        let token = UserToken::from_existing(
                            &reqwest::Client::new(),
                            AccessToken::new("mock_access_token".to_string()),
                            None,
                            None,
                        )
                        .await
                        .unwrap();

                        let msg = UiMessage::LoggedIn {
                            token: Arc::new(token),
                            user_id: "12345".to_string().into(),
                            user_login: "twitchdev".to_string(),
                        };
                        let _ = tx.send(msg).await;
                    });
                }

                if let Some(error_message) = error {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(error_message.clone()).color(egui::Color32::RED));
                }
            });
        }
    }

    fn draw_logged_in(&mut self, ctx: &egui::Context) {
        // First, process incoming messages outside the UI closure
        if let AppState::LoggedIn { message_rx, chat_messages, .. } = &mut self.state {
            while let Ok(message) = message_rx.try_recv() {
                chat_messages.push(message);
                // Keep only the last 100 messages to prevent memory issues
                if chat_messages.len() > 100 {
                    chat_messages.remove(0);
                }
            }
        }

        // Now extract what we need for the UI
        let (user_login, current_channel, chat_messages_len) = match &self.state {
            AppState::LoggedIn {
                user_login,
                current_channel,
                chat_messages,
                ..
            } => (user_login.clone(), current_channel.clone(), chat_messages.len()),
            _ => return,
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!("Logged in as {}", user_login));
            ui.add_space(20.0);

            // Channel joining section
            ui.horizontal(|ui| {
                // Get mutable references for the input fields
                if let AppState::LoggedIn {
                    channel_to_join,
                    current_channel,
                    token,
                    user_id,
                    message_tx,
                    chat_client,
                    ..
                } = &mut self.state {
                    ui.label("Join Channel:");
                    ui.text_edit_singleline(channel_to_join);
                    if ui.button("Join").clicked() && !channel_to_join.is_empty() {
                        *current_channel = Some(channel_to_join.clone());
                        let event_sub_tx = message_tx.clone();
                        let token_clone = token.clone();
                        let channel_login_clone = channel_to_join.clone();
                        let user_id_clone = user_id.clone();
                        let ui_tx = self.ui_message_tx.clone();
                        let chat_client_clone = chat_client.clone();
                        
                        self.tokio_runtime_handle.spawn(async move {
                            // Get broadcaster ID
                            match chat_client_clone.get_user_id(&channel_login_clone, &token_clone).await {
                                Ok(Some(broadcaster_id)) => {
                                    tracing::info!("Broadcaster ID: {}", broadcaster_id);
                                    
                                    let client = EventSubClient::new(user_id_clone, token_clone, event_sub_tx);
                                    if let Err(e) = client.run(channel_login_clone).await {
                                        tracing::error!("EventSub client failed: {}", e);
                                        let _ = ui_tx.send(UiMessage::Error(format!("Failed to connect to chat: {}", e))).await;
                                    }
                                }
                                Ok(None) => {
                                    let _ = ui_tx.send(UiMessage::Error("Channel not found".to_string())).await;
                                }
                                Err(e) => {
                                    let _ = ui_tx.send(UiMessage::Error(format!("Failed to get channel info: {}", e))).await;
                                }
                            }
                        });
                    }
                }
            });

            ui.add_space(10.0);
            ui.label(format!(
                "Current Channel: {}",
                current_channel.as_deref().unwrap_or("None")
            ));

            // Chat messages area
            ui.add_space(10.0);
            ui.label("Chat Messages:");
            ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(300.0)
                .show(ui, |ui| {
                    if let AppState::LoggedIn { chat_messages, .. } = &self.state {
                        for message in chat_messages.iter() {
                            ui.label(message);
                        }
                    }
                });

            // Message input section
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if let AppState::LoggedIn {
                    message_to_send,
                    current_channel,
                    send_in_progress,
                    last_error,
                    ..
                } = &mut self.state {
                    ui.label("Message:");
                    ui.text_edit_multiline(message_to_send);
                    
                    let can_send = !message_to_send.is_empty() && 
                                   current_channel.is_some() && 
                                   !*send_in_progress;
                    
                    // Clone the message for the button handlers
                    let message_for_send = message_to_send.clone();
                    let message_for_announcement = message_to_send.clone();
                    
                    ui.add_enabled_ui(can_send, |ui| {
                        if ui.button("Send").clicked() {
                            if let Err(e) = ChatClient::validate_message(&message_for_send) {
                                *last_error = Some(format!("Message validation failed: {}", e));
                            } else {
                                // We need to call send_message outside this closure
                                // Store the intent and handle it after the UI update
                                self.send_message(message_for_send, false);
                            }
                        }
                        
                        if ui.button("Send Announcement").clicked() {
                            if let Err(e) = ChatClient::validate_message(&message_for_announcement) {
                                *last_error = Some(format!("Message validation failed: {}", e));
                            } else {
                                self.send_message(message_for_announcement, true);
                            }
                        }
                    });
                    
                    if *send_in_progress {
                        ui.spinner();
                    }

                    // Error display
                    if let Some(error) = last_error {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(format!("Error: {}", error)).color(egui::Color32::RED));
                        if ui.button("Clear Error").clicked() {
                            *last_error = None;
                        }
                    }
                }
            });

            // Connection status
            ui.add_space(10.0);
            ui.separator();
            ui.label(format!("Messages in chat: {}", chat_messages_len));
        });
    }

    fn send_message(&mut self, message: String, is_announcement: bool) {
        if let AppState::LoggedIn {
            token,
            user_id,
            current_channel,
            chat_client,
            send_in_progress,
            ..
        } = &mut self.state
        {
            if let Some(channel) = current_channel.clone() {
                *send_in_progress = true;
                let token_clone = token.clone();
                let user_id_clone = user_id.clone();
                let chat_client_clone = chat_client.clone();
                let ui_tx = self.ui_message_tx.clone();

                self.tokio_runtime_handle.spawn(async move {
                    // First get the broadcaster ID if we don't have it
                    let broadcaster_id = match chat_client_clone.get_user_id(&channel, &token_clone).await {
                        Ok(Some(id)) => id,
                        Ok(None) => {
                            let _ = ui_tx.send(UiMessage::MessageSendError("Channel not found".to_string())).await;
                            return;
                        }
                        Err(e) => {
                            let _ = ui_tx.send(UiMessage::MessageSendError(format!("Failed to get channel info: {}", e))).await;
                            return;
                        }
                    };

                    let result = if is_announcement {
                        chat_client_clone
                            .send_announcement(
                                broadcaster_id.as_ref(),
                                user_id_clone.as_ref(),
                                &message,
                                Some(AnnouncementColor::Primary),
                                &token_clone,
                            )
                            .await
                    } else {
                        chat_client_clone
                            .send_chat_message(
                                broadcaster_id.as_ref(),
                                user_id_clone.as_ref(),
                                &message,
                                &token_clone,
                            )
                            .await
                    };

                    match result {
                        Ok(_) => {
                            let _ = ui_tx.send(UiMessage::MessageSent).await;
                        }
                        Err(e) => {
                            let _ = ui_tx.send(UiMessage::MessageSendError(format!("Failed to send message: {}", e))).await;
                        }
                    }
                });
            }
        }
    }
}