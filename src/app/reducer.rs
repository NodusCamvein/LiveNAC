use super::state::AppState;
use crate::{
    app::config::Config,
    core::{auth::AuthMessage, chat::ChatClient},
    emotes::twitch_api::TwitchApiClient,
    events::app_event::{AppEvent, ChatEvent},
    models::{message::MessageFragment, user::User},
};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc;
use twitch_oauth2::UserToken;

pub fn reduce(
    state: &mut AppState,
    event: AppEvent,
    config: &mut Config,
    event_tx: mpsc::Sender<AppEvent>,
) {
    match event {
        AppEvent::ConfigLoaded(config_result) => {
            handle_config_loaded(config_result, state, config);
        }
        AppEvent::SilentLoginComplete(result) => {
            handle_silent_login_complete(state, result, config, event_tx.clone());
        }
        AppEvent::ProfileSwitchSilentLoginComplete(result, profile_name) => match result {
            Ok(token) => {
                handle_successful_login(state, token, config, event_tx, Some(profile_name))
            }
            Err(e) => {
                tracing::warn!(
                    "Silent login for profile '{}' failed: {}. Proceeding to interactive flow.",
                    profile_name,
                    e
                );
                *state = AppState::RequestingInteractiveLogin { profile_name };
            }
        },
        AppEvent::Auth(auth_message) => {
            handle_auth_message(state, auth_message, config, event_tx.clone());
        }
        AppEvent::AuthCancel => {
            // This is now only used if the user closes the window during first time setup
            // or other non-post-login flows.
            // The interactive login flow can't be cancelled from the app side anymore.
        }
        AppEvent::AuthFlowStartFailed(err) => {
            // If the auth flow fails to even start, the best we can do is go back
            // to the setup screen and display an error.
            *state = AppState::FirstTimeSetup {
                client_id_input: config.client_id.clone().unwrap_or_default(),
                client_secret_input: String::new(),
                profile_name_input: String::new(),
                error: Some(err),
            };
        }
        AppEvent::Chat(chat_message) => {
            handle_chat_message(state, chat_message);
        }
        AppEvent::GlobalEmotesLoaded(result) => {
            if let AppState::LoggedIn { global_emotes, .. } = state {
                match result {
                    Ok(emotes) => {
                        *global_emotes = emotes;
                        tracing::info!(
                            "Successfully loaded {} global emotes.",
                            global_emotes.len()
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to load global emotes: {}", e);
                    }
                }
            }
        }
    }
}

fn handle_config_loaded(
    result: Result<Config, eyre::Report>,
    state: &mut AppState,
    config: &mut Config,
) {
    match result {
        Ok(loaded_config) => {
            *config = loaded_config;
            *state = AppState::Initializing {
                task_spawned: false,
            };
        }
        Err(e) => {
            tracing::error!("Failed to load config: {}", e);
            // If config fails to load, we can't do anything else.
            // Go to first time setup.
            *state = AppState::FirstTimeSetup {
                client_id_input: String::new(),
                client_secret_input: String::new(),
                profile_name_input: String::new(),
                error: Some(format!("Failed to load config: {}", e)),
            };
        }
    }
}

fn handle_silent_login_complete(
    state: &mut AppState,
    result: Result<UserToken, eyre::Report>,
    config: &mut Config,
    event_tx: mpsc::Sender<AppEvent>,
) {
    match result {
        Ok(token) => {
            handle_successful_login(state, token, config, event_tx, None);
        }
        Err(e) => {
            tracing::info!("Silent login failed: {}", e);
            if config.profiles.is_empty() {
                tracing::info!("No profiles found. Proceeding to first time setup.");
                *state = AppState::FirstTimeSetup {
                    client_id_input: String::new(),
                    client_secret_input: String::new(),
                    profile_name_input: String::new(),
                    error: None,
                };
            } else {
                tracing::info!(
                    "Profiles found, but silent login failed. Proceeding to logged out state."
                );
                *state = AppState::LoggedOut {
                    show_profile_manager_on_entry: true,
                };
            }
        }
    }
}

fn handle_auth_message(
    state: &mut AppState,
    msg: AuthMessage,
    config: &mut Config,
    event_tx: mpsc::Sender<AppEvent>,
) {
    match msg {
        AuthMessage::Success(token) => {
            // The profile name should already be set in the config from the UI logic
            let profile_name = config.active_profile_name.clone();
            handle_successful_login(state, token, config, event_tx, profile_name);
        }
        AuthMessage::Error(err) => {
            // If auth fails, we go back to the first time setup screen to show the error.
            // This covers cases where the user denies auth in the browser.
            *state = AppState::FirstTimeSetup {
                client_id_input: config.client_id.clone().unwrap_or_default(),
                client_secret_input: config.client_secret.clone().unwrap_or_default(),
                profile_name_input: config.active_profile_name.clone().unwrap_or_default(),
                error: Some(format!("Authentication Failed: {}", err)),
            };
        }
    }
}

fn handle_successful_login(
    state: &mut AppState,
    token: UserToken,
    config: &mut Config,
    event_tx: mpsc::Sender<AppEvent>,
    auth_profile_name: Option<String>,
) {
    let user_id = token.user_id.clone();
    let user_login = token.login.clone();

    if let Some(name) = auth_profile_name {
        config.active_profile_name = Some(name);
    }

    if let Some(name) = &config.active_profile_name {
        if let Some(profile) = config.profiles.iter_mut().find(|p| &p.name == name) {
            if profile.twitch_user_id.is_none() {
                profile.twitch_user_id = Some(user_id.to_string());
            }
        }
    } else {
        let new_profile_name = user_login.to_string();
        config.profiles.push(crate::app::config::Profile {
            name: new_profile_name.clone(),
            twitch_user_id: Some(user_id.to_string()),
        });
        config.active_profile_name = Some(new_profile_name);
    }

    let config_to_save = config.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::app::config::save(&config_to_save).await {
            tracing::error!("Failed to save config after login: {}", e);
        }
    });

    let token = Arc::new(token);

    *state = AppState::LoggedIn {
        token: token.clone(),
        user_id,
        user_login: user_login.to_string(),
        channel_to_join: String::new(),
        current_channel: None,
        message_to_send: String::new(),
        chat_messages: Vec::new(),
        users: HashSet::new(),
        global_emotes: Vec::new(),
        chat_client: ChatClient::new(),
        send_in_progress: false,
        last_error: None,
        eventsub_task: None,
    };

    if let Some(client_id) = &config.client_id {
        let twitch_api_client = TwitchApiClient::new(client_id.clone());
        let token_clone = token.clone();
        tokio::spawn(async move {
            let emotes_result = twitch_api_client.get_global_emotes(&token_clone).await;
            let event = AppEvent::GlobalEmotesLoaded(emotes_result.map_err(|e| e.to_string()));
            let _ = event_tx.send(event).await;
        });
    } else {
        tracing::error!("Client ID not found, cannot fetch global emotes.");
    }
}

fn handle_chat_message(state: &mut AppState, msg: ChatEvent) {
    if let AppState::LoggedIn {
        chat_messages,
        users,
        send_in_progress,
        last_error,
        message_to_send,
        ..
    } = state
    {
        match msg {
            ChatEvent::NewChatMessage(mut message) => {
                if let Some(MessageFragment::Text(text)) = message.fragments.last_mut() {
                    *text = text.trim_end().to_string();
                }

                let user = User {
                    name: message.sender_name.clone(),
                    color: message.sender_color,
                };
                users.insert(user);
                chat_messages.push(message);
            }
            ChatEvent::MessageSent => {
                *send_in_progress = false;
                message_to_send.clear();
            }
            ChatEvent::MessageSendError(err) => {
                *send_in_progress = false;
                *last_error = Some(err);
            }
            ChatEvent::EventSubError(err) => {
                *last_error = Some(format!("Chat connection error: {}", err));
            }
        }
        if chat_messages.len() > 200 {
            chat_messages.remove(0);
        }
    }
}
