use super::state::AppState;
use crate::{
    app::config::Config,
    core::{auth::AuthMessage, chat::ChatClient},
    events::app_event::{AppEvent, ChatEvent},
    models::user::User,
};
use std::{collections::HashSet, sync::Arc};

pub fn reduce(state: &mut AppState, event: AppEvent) {
    match event {
        AppEvent::ConfigLoaded(config_result) => {
            handle_config_loaded(state, config_result);
        }
        AppEvent::Auth(auth_message) => {
            handle_auth_message(state, auth_message);
        }
        AppEvent::Chat(chat_message) => {
            handle_chat_message(state, chat_message);
        }
    }
}

fn handle_config_loaded(state: &mut AppState, result: Result<Config, eyre::Report>) {
    match result {
        Ok(config) => {
            let client_id = config.client_id.clone();
            // TODO: Config should be part of the app state
            // self.config = config;
            if let Some(_id) = client_id {
                // TODO: This should be an action/event
                // self.start_authentication(id);
                *state = AppState::Authenticating {
                    status_message: "Authenticating...".to_string(),
                };
            } else {
                *state = AppState::FirstTimeSetup {
                    client_id_input: String::new(),
                    error: None,
                };
            }
        }
        Err(e) => {
            *state = AppState::FirstTimeSetup {
                client_id_input: String::new(),
                error: Some(format!("Failed to load config: {}", e)),
            }
        }
    }
}

fn handle_auth_message(state: &mut AppState, msg: AuthMessage) {
    if let AppState::Authenticating { .. } = state {
        match msg {
            AuthMessage::Success(token) => {
                let user_id = token.user_id.clone();
                let user_login = token.login.to_string();
                *state = AppState::LoggedIn {
                    token: Arc::new(token),
                    user_id,
                    user_login,
                    channel_to_join: String::new(),
                    current_channel: None,
                    message_to_send: String::new(),
                    chat_messages: Vec::new(),
                    users: HashSet::new(),
                    chat_client: ChatClient::new(),
                    send_in_progress: false,
                    last_error: None,
                    eventsub_task: None,
                };
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
