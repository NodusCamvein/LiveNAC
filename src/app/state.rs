use crate::{
    core::chat::ChatClient,
    models::{message::ChatMessage, user::User},
};
use std::{collections::HashSet, sync::Arc};
use tokio::task::JoinHandle;
use twitch_oauth2::UserToken;
use twitch_types::UserId;

/// Holds the URI and code for the device flow.
#[derive(Clone, Debug, Default)]
pub struct DeviceFlowInfo {
    pub uri: String,
    pub user_code: String,
}

/// Represents the various states of the application's lifecycle.
pub enum AppState {
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
        chat_messages: Vec<ChatMessage>,
        users: HashSet<User>,
        chat_client: ChatClient,
        send_in_progress: bool,
        last_error: Option<String>,
        eventsub_task: Option<JoinHandle<()>>,
    },
}
