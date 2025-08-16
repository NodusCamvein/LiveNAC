use super::state::AppState;
use crate::{
    app::config::Config,
    core::{auth::AuthMessage, chat::ChatClient},
    emotes::twitch_api::TwitchApiClient,
    events::app_event::{AppEvent, ChatEvent},
    models::user::User,
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
            handle_config_loaded(config_result, config);
        }
        AppEvent::SilentLoginComplete(result) => {
            handle_silent_login_complete(state, result, config, event_tx);
        }
        AppEvent::Auth(auth_message) => {
            handle_auth_message(state, auth_message, config, event_tx);
        }
        AppEvent::Chat(chat_message) => {
            handle_chat_message(state, chat_message);
        }
        AppEvent::GlobalEmotesLoaded(result) => {
            if let AppState::LoggedIn { global_emotes, .. } = state {
                match result {
                    Ok(emotes) => {
                        *global_emotes = emotes;
                        tracing::info!("Successfully loaded {} global emotes.", global_emotes.len());
                    }
                    Err(e) => {
                        tracing::error!("Failed to load global emotes: {}", e);
                    }
                }
            }
        }
    }
}

fn handle_config_loaded(result: Result<Config, eyre::Report>, config: &mut Config) {
    if let Ok(loaded_config) = result {
        *config = loaded_config;
    }
    // If config loading fails, the app will proceed to the FirstTimeSetup state
    // driven by the SilentLoginComplete event failing.
}

fn handle_silent_login_complete(
    state: &mut AppState,
    result: Result<UserToken, eyre::Report>,
    config: &Config,
    event_tx: mpsc::Sender<AppEvent>,
) {
    match result {
        Ok(token) => {
            handle_successful_login(state, token, config, event_tx);
        }
        Err(e) => {
            tracing::info!("Silent login failed: {}. Proceeding to first time setup.", e);
            *state = AppState::FirstTimeSetup {
                client_id_input: String::new(),
                error: None, // Don't show an error here, it's expected on first launch.
            };
        }
    }
}

fn handle_auth_message(
    state: &mut AppState,
    msg: AuthMessage,
    config: &Config,
    event_tx: mpsc::Sender<AppEvent>,
) {
    if let AppState::Authenticating { .. } = state {
        match msg {
            AuthMessage::Success(token) => {
                handle_successful_login(state, token, config, event_tx);
            }
            AuthMessage::Error(err) => {
                *state = AppState::FirstTimeSetup {
                    client_id_input: String::new(),
                    error: Some(format!("Authentication Failed: {}", err)),
                }
            }
        }
    }
}

fn handle_successful_login(
    state: &mut AppState,
    token: UserToken,
    config: &Config,
    event_tx: mpsc::Sender<AppEvent>,
) {
    let user_id = token.user_id.clone();
    let user_login = token.login.to_string();
    let token = Arc::new(token);

    *state = AppState::LoggedIn {
        token: token.clone(),
        user_id,
        user_login,
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
            ChatEvent::NewChatMessage(message) => {
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
