use super::state::{AppState, DeviceFlowInfo};
use crate::app::config::Config;
use crate::core::auth::AuthMessage;
use crate::core::chat::ChatClient;
use crate::events::app_event::{AppEvent, ChatEvent};
use std::sync::Arc;

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
                    device_flow_info: None,
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
    if let AppState::Authenticating {
        status_message,
        device_flow_info,
    } = state
    {
        match msg {
            AuthMessage::AwaitingDeviceActivation { uri, user_code } => {
                *status_message = "Please authorize in your browser.".to_string();
                *device_flow_info = Some(DeviceFlowInfo { uri, user_code });
            }
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
        send_in_progress,
        last_error,
        message_to_send,
        ..
    } = state
    {
        match msg {
            ChatEvent::NewChatMessage(message) => chat_messages.push(message),
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
