use crate::{
    app::{
        config::{self, Config, Profile},
        reducer,
        state::AppState,
    },
    core::{
        auth::AuthClient,
        chat::{AnnouncementColor, ChatClient},
        eventsub::EventSubClient,
    },
    events::app_event::{AppEvent, ChatEvent},
    ui::{
        chat::{chat_bar, chat_log, emote_picker, user_list},
        profiles,
    },
};
use eframe::egui::{self, Align, FontDefinitions, Key, Layout, RichText, SidePanel, TopBottomPanel};
use fontdb;
use tokio::sync::mpsc;

pub struct App {
    state: AppState,
    event_rx: mpsc::Receiver<AppEvent>,
    event_tx: mpsc::Sender<AppEvent>,
    config: Config,
    show_settings_window: bool,
    show_profile_manager: bool,
    new_profile_name: String,
    profile_to_remove_name: String,
    profile_manager_error: Option<String>,
    show_toolbar: bool,
    show_emote_picker: bool,
    show_user_list: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let (event_tx, event_rx) = mpsc::channel(100);
        let default_config = Config::default();

        // --- FONT SETUP ---
        let mut fonts = FontDefinitions::default();

        /*
        // 1. Start with the bundled font as a base.
        fonts.font_data.insert(
            "noto_sans_jp".to_owned(),
            egui::FontData::from_static(include_bytes!(
                "../../assets/fonts/NotoSansJP-Regular.otf"
            ))
            .into(),
        );

        // Add it to the proportional family.
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "noto_sans_jp".to_owned());
        */

        // 2. Load system fonts
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        // 3. Define a list of fonts to search for, in order of preference.
        let fonts_to_load = [
            // CJK
            "Microsoft YaHei",
            "PingFang SC",
            "Noto Sans CJK SC",
            "WenQuanYi Zen Hei",
            "Yu Gothic",
            "Hiragino Sans",
            "Noto Sans CJK JP",
            "Malgun Gothic",
            "Apple SD Gothic Neo",
            "Noto Sans CJK KR",
            // Generic Fallbacks
            "Arial",
            "Helvetica",
            "Cantarell",
            "Ubuntu",
        ];

        let mut loaded_font_count = 0;
        for font_name in fonts_to_load.iter() {
            let query = fontdb::Query {
                families: &[fontdb::Family::Name(font_name)],
                ..Default::default()
            };

            if let Some(font_id) = db.query(&query) {
                if let Some(font_source) = db.face_source(font_id) {
                    let font_data: Option<std::borrow::Cow<[u8]>> = match font_source.0 {
                        fontdb::Source::Binary(data) => {
                            Some(std::borrow::Cow::Owned(data.as_ref().as_ref().to_vec()))
                        }
                        fontdb::Source::File(path) => {
                            std::fs::read(path).ok().map(std::borrow::Cow::Owned)
                        }
                        fontdb::Source::SharedFile(path, _) => {
                            std::fs::read(path).ok().map(std::borrow::Cow::Owned)
                        }
                    };

                    if let Some(font_data) = font_data {
                        let font_name_string = font_name.to_string();
                        let egui_font_name =
                            format!("system-{}", font_name_string.to_lowercase().replace(' ', "_"));

                        fonts.font_data.insert(
                            egui_font_name.clone(),
                            egui::FontData::from_owned(font_data.to_vec()).into(),
                        );

                        fonts
                            .families
                            .entry(egui::FontFamily::Proportional)
                            .or_default()
                            .push(egui_font_name.clone());

                        tracing::info!("Loaded system font: {}", font_name);
                        loaded_font_count += 1;
                    }
                }
            }
        }

        if loaded_font_count == 0 {
            tracing::warn!(
                "No preferred CJK or fallback fonts found on the system. Default fonts will be used."
            );
        }

        // --- END FONT SETUP ---
        cc.egui_ctx.set_fonts(fonts);

        Self {
            state: AppState::Startup {
                task_spawned: false,
            },
            event_rx,
            event_tx,
            config: default_config.clone(),
            show_settings_window: false,
            show_profile_manager: false,
            new_profile_name: String::new(),
            profile_to_remove_name: String::new(),
            profile_manager_error: None,
            show_toolbar: false,
            show_emote_picker: false,
            show_user_list: false,
        }
    }
}

use eyre::eyre;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_settings(ctx);

        while let Ok(event) = self.event_rx.try_recv() {
            reducer::reduce(&mut self.state, event, &mut self.config, self.event_tx.clone());
        }

        let mut send_action: Option<bool> = None;
        let mut login_action: Option<bool> = None;

        let mut cancel_auth_action = false;

        match &mut self.state {
            AppState::Startup { task_spawned } => {
                if !*task_spawned {
                    *task_spawned = true;
                    let tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        let config_result = config::load().await;
                        let config = match config_result {
                            Ok(c) => {
                                tx.send(AppEvent::ConfigLoaded(Ok(c.clone()))).await.ok();
                                c
                            }
                            Err(e) => {
                                tx.send(AppEvent::ConfigLoaded(Err(e))).await.ok();
                                tx.send(AppEvent::SilentLoginComplete(Err(eyre!(
                                    "Failed to load config"
                                ))))
                                .await
                                .ok();
                                return;
                            }
                        };

                        if let Some(client_id) = config.client_id.clone() {
                            let active_profile_name = config.active_profile_name.clone();
                            match AuthClient::new(client_id, tx.clone(), active_profile_name).await
                            {
                                Ok(auth_client) => {
                                    let result = auth_client.try_silent_login().await;
                                    tx.send(AppEvent::SilentLoginComplete(result)).await.ok();
                                }
                                Err(e) => {
                                    tx.send(AppEvent::SilentLoginComplete(Err(e))).await.ok();
                                }
                            }
                        } else {
                            tx.send(AppEvent::SilentLoginComplete(Err(eyre!(
                                "Client ID not configured"
                            ))))
                            .await
                            .ok();
                        }
                    });
                }
                self.draw_loading_ui(ctx, "Starting...");
            }
            AppState::FirstTimeSetup { .. } => self.draw_first_time_setup(ctx, &mut login_action),
            AppState::WaitingForToken { .. } => {
                self.draw_waiting_for_token_ui(ctx, &mut cancel_auth_action)
            }
            AppState::LoggedIn { .. } => self.draw_logged_in(ctx, &mut send_action),
        }

        // Refactored action handling to be outside the main state match
        if let Some(is_announcement) = send_action {
            self.send_message(is_announcement);
        }
        if let Some(true) = login_action {
            self.handle_login_action();
        }
        if cancel_auth_action {
            // Use try_send to avoid blocking the UI thread.
            self.event_tx.try_send(AppEvent::AuthCancel).ok();
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl App {
    fn handle_login_action(&mut self) {
        let profile_name = self.config.active_profile_name.clone();
        self.trigger_interactive_login(profile_name);
    }

    fn trigger_interactive_login(&mut self, profile_name: Option<String>) {
        let client_id = self.config.client_id.clone().unwrap_or_default();
        let tx = self.event_tx.clone();
        let profile_name_clone = profile_name.clone();

        tokio::spawn(async move {
            let auth_client = match AuthClient::new(client_id, tx.clone(), profile_name_clone).await
            {
                Ok(client) => client,
                Err(e) => {
                    tx.send(AppEvent::AuthFlowStartFailed(format!(
                        "Failed to create auth client: {}",
                        e
                    )))
                    .await
                    .ok();
                    return;
                }
            };

            if let Err(e) = auth_client.start_interactive_login().await {
                tx.send(AppEvent::AuthFlowStartFailed(format!(
                    "Failed to start interactive login: {}",
                    e
                )))
                .await
                .ok();
            }
        });

        self.state = AppState::WaitingForToken {
            profile_name,
            token_input: String::new(),
            error: None,
        };
    }

    fn apply_settings(&mut self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size = self.config.font_size;
        });
        ctx.set_style(style);
    }

    fn draw_loading_ui(&self, ctx: &egui::Context, message: &str) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.heading(message);
                    ui.add_space(10.0);
                    ui.spinner();
                });
            });
        });
    }

    fn draw_first_time_setup(&mut self, ctx: &egui::Context, login_action: &mut Option<bool>) {
        if let AppState::FirstTimeSetup {
            client_id_input,
            profile_name_input,
            error,
        } = &mut self.state
        {
            let mut profile_input_resp = None;
            let mut client_id_input_resp = None;

            egui::CentralPanel::default().show(ctx, |ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(ui.available_height() * 0.2);
                    ui.heading("LiveNAC");
                    ui.label("Twitch Chat Client");
                    ui.label("First-Time Setup");
                    ui.add_space(20.0);

                    ui.label("Profile Name:");
                    profile_input_resp = Some(ui.text_edit_singleline(profile_name_input));
                    ui.add_space(10.0);

                    let client_id_exists = self.config.client_id.is_some();
                    if !client_id_exists {
                        ui.label("Twitch Application Client ID:");
                        client_id_input_resp = Some(ui.text_edit_singleline(client_id_input));
                    } else {
                        ui.label(RichText::new("Client ID found in config.").italics());
                    }
                });

                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.add_space(ui.available_height() * 0.4);

                    let enter_pressed = (profile_input_resp.as_ref().unwrap().lost_focus()
                        || client_id_input_resp
                            .as_ref()
                            .map_or(false, |r| r.lost_focus()))
                        && ctx.input(|i| i.key_pressed(Key::Enter));

                    if ui.button("Login with Twitch").clicked() || enter_pressed {
                        let client_id_exists = self.config.client_id.is_some();
                        if !client_id_exists {
                            if client_id_input.is_empty() {
                                *error = Some("Client ID cannot be empty.".to_string());
                                return;
                            }
                            // Set the client_id in the config if it was just entered.
                            self.config.client_id = Some(client_id_input.clone());
                        }

                        if profile_name_input.is_empty() {
                            *error = Some("Profile Name cannot be empty.".to_string());
                            return;
                        }
                        if self.config.profiles.iter().any(|p| p.name == *profile_name_input) {
                            *error = Some("A profile with this name already exists.".to_string());
                            return;
                        }

                        self.config.active_profile_name = Some(profile_name_input.clone());
                        self.config.profiles.push(Profile {
                            name: profile_name_input.clone(),
                            twitch_user_id: None,
                        });

                        *login_action = Some(true);
                    }
                    if let Some(err) = error {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                });
            });
        }
    }

    fn draw_waiting_for_token_ui(&mut self, ctx: &egui::Context, cancel_auth_action: &mut bool) {
        if let AppState::WaitingForToken {
            token_input,
            error,
            ..
        } = &mut self.state
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        ui.heading("Awaiting Token");
                        ui.label("Please copy the token from your browser and paste it below.");
                        ui.add_space(10.0);

                        let token_input_field = ui.text_edit_singleline(token_input);

                        let submit_clicked = ui.button("Submit Token").clicked();
                        let enter_pressed = token_input_field.lost_focus()
                            && ctx.input(|i| i.key_pressed(Key::Enter));

                        if submit_clicked || enter_pressed {
                            self.event_tx
                                .try_send(AppEvent::TokenPasted(token_input.clone()))
                                .ok();
                        }

                        if ui.button("Cancel").clicked() {
                            *cancel_auth_action = true;
                        }

                        if let Some(err) = error {
                            ui.add_space(10.0);
                            ui.colored_label(egui::Color32::RED, err);
                        }
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
                            if ui.button("Exit").clicked() {
                                std::process::exit(0);
                            }
                        });
                        if ui.button("Settings").clicked() {
                            self.show_settings_window = true;
                        }
                        if ui.button("Profiles").clicked() {
                            self.show_profile_manager = true;
                        }
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
                    emote_picker::draw_emote_picker(ui, &mut self.state, &self.config);
                    ui.separator();
                }
                chat_bar::draw_chat_bar(
                    ui,
                    &mut self.state,
                    send_action,
                    &mut self.show_emote_picker,
                    &mut self.show_user_list,
                );
            });

            if self.show_user_list {
                SidePanel::right("user_list_panel")
                    .min_width(150.0)
                    .default_width(180.0)
                    .show(ctx, |ui| {
                        user_list::draw_user_list(ui, &mut self.state);
                    });
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                chat_log::draw_chat_log(ui, &mut self.state, &self.config);
            });

            self.draw_settings_window(ctx);
            self.draw_profile_manager_window(ctx);
        }
    }

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings")
            .open(&mut self.show_settings_window)
            .show(ctx, |ui| {
                ui.heading("Appearance");
                let mut config_changed = false;

                config_changed |= ui
                    .add(egui::Slider::new(&mut self.config.font_size, 8.0..=24.0).text("Font Size"))
                    .changed();

                config_changed |= ui
                    .add(
                        egui::Slider::new(&mut self.config.emote_size, 16.0..=64.0)
                            .text("Emote Size"),
                    )
                    .changed();
                
                config_changed |= ui
                    .checkbox(&mut self.config.collapse_emotes, "Collapse space between emotes")
                    .changed();

                config_changed |= ui
                    .checkbox(&mut self.config.show_timestamps, "Show Timestamps")
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

    fn draw_profile_manager_window(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_profile_manager;
        egui::Window::new("Profile Manager")
            .open(&mut is_open)
            .show(ctx, |ui| {
                if let Some(action) = profiles::draw_profile_manager(
                    ctx,
                    ui,
                    &self.config,
                    &mut self.new_profile_name,
                    &mut self.profile_to_remove_name,
                    &self.profile_manager_error,
                ) {
                    match action {
                        profiles::ProfileManagerAction::Login(name) => {
                            self.profile_manager_error = None;
                            self.trigger_interactive_login(Some(name));
                        }
                        profiles::ProfileManagerAction::Add(name) => {
                            if !self.config.profiles.iter().any(|p| p.name == name) {
                                self.profile_manager_error = None;
                                self.config.profiles.push(Profile {
                                    name,
                                    twitch_user_id: None,
                                });
                                let config_to_save = self.config.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = config::save(&config_to_save).await {
                                        tracing::error!("Failed to save config: {}", e);
                                    }
                                });
                            } else {
                                self.profile_manager_error =
                                    Some("A profile with this name already exists.".to_string());
                            }
                        }
                        profiles::ProfileManagerAction::Remove(name) => {
                            self.config.profiles.retain(|p| p.name != name);
                            if self.config.active_profile_name.as_ref() == Some(&name) {
                                self.config.active_profile_name = None;
                            }
                            let config_to_save = self.config.clone();
                            tokio::spawn(async move {
                                if let Err(e) = config::save(&config_to_save).await {
                                    tracing::error!("Failed to save config: {}", e);
                                }
                            });
                        }
                    }
                }
            });
        self.show_profile_manager = is_open;
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