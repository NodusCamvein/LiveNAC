use crate::{
    core::chat::ChatClient,
    emotes::twitch_api::TwitchEmote,
    models::{message::ChatMessage, user::User},
};
use std::{collections::HashSet, sync::Arc};
use tokio::task::JoinHandle;
use twitch_oauth2::UserToken;
use twitch_types::UserId;

/// Represents the various states of the application's lifecycle.
pub enum AppState {
    Startup {
        task_spawned: bool,
    },
    FirstTimeSetup {
        client_id_input: String,
        client_secret_input: String,
        profile_name_input: String,
        error: Option<String>,
    },
    ProfileSelection {
        error: Option<String>,
    },
    RequestingInteractiveLogin {
        profile_name: String,
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
        global_emotes: Vec<TwitchEmote>,
        chat_client: ChatClient,
        send_in_progress: bool,
        last_error: Option<String>,
        eventsub_task: Option<JoinHandle<()>>,
    },
}
