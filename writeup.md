LiveNAC: A Simple Twitch Chat Client in Rust

LiveNAC is a Rust-based Twitch chat client inspired by Chatterino, using Helix and EventSub APIs to replace deprecated IRC. It features a login screen for Client ID and OAuth, followed by a UI to join channels, view, and send messages, including announcements, via an EGUI interface.
App Flow

    Open: Window with Client ID field and "Login to Twitch" button.

    Enter Client ID, click login: Opens browser for OAuth, displaying user code/URI.

    Authorize: Switches to logged-in state.

    Top: Channel name field with "Join Channel" button.

    Middle: Scrollable chat messages.

    Bottom: Message field with "Send" and "Send Announcement" buttons (Enter key sends messages).

This guide details setup, authentication, and functionality using Cargo.toml dependencies, focusing on twitch_api and twitch_oauth2 crates. It includes OAuth2 examples and integrates announcement and ban handling.
Prerequisites

    Rust: Version 1.89.

    Twitch Developer Console Account: Create an app for a Client ID (no redirect URI for device code flow).

    Scopes: user:read:chat, user:write:chat, chat:read, chat:edit, user:read:emotes, moderator:read:chatters, moderator:manage:announcements, moderator:read:bans (for bans).

    No External Installs: All libraries in Cargo.toml.

Cargo.toml Setup

Create Cargo.toml:

[package]
name = "livenac"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1.77"
eframe = "0.32.0"
egui = "0.32.0"
eyre = "0.6.11"
futures = "0.3.30"
parking_lot = "0.12.1"
reqwest = { version = "0.12.22", features = ["json"] }
serde = { version = "1.0.197", features = ["derive"] }
serde_json = "1.0.114"
tokio = { version = "1.36.0", features = ["full"] }
tracing = "0.1.40"
tracing-subscriber = "0.3.18"
twitch_api = { version = "0.7.2", features = ["helix", "reqwest", "eventsub"] }
twitch_oauth2 = { version = "0.15.2", features = ["reqwest"] }
twitch_types = "0.4.8"
url = "2.5.0"

Run cargo build. The twitch_api crate enables Helix/EventSub with helix, reqwest, eventsub features. The twitch_oauth2 crate handles authentication.
Project Structure

    src/main.rs: Initializes Tokio runtime and EGUI.

    src/chat.rs: Helix API for user IDs, sending messages, announcements, and fetching bans.

    src/eventsub.rs: EventSub WebSocket for messages and subscription management.

    src/ui.rs: EGUI UI, state management, OAuth flow.

Authentication (Using twitch_oauth2)

LiveNAC uses device code flow for authentication.
Device Code Flow

This example from examples/device_code_flow.rs demonstrates the process of using DeviceUserTokenBuilder to obtain a user token.

//! Example of how to create a user token using device code flow.
//! The device code flow can be used on confidential and public clients.
use twitch_oauth2::{DeviceUserTokenBuilder, TwitchToken, UserToken};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenv::dotenv(); // Eat error
    let mut args = std::env::args().skip(1);

    // Setup the http client to use with the library.
    let reqwest = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    // Grab the client id, convert to a `ClientId` with the `new` method.
    let client_id = get_env_or_arg("TWITCH_CLIENT_ID", &mut args)
        .map(twitch_oauth2::ClientId::new)
        .expect("Please set env: TWITCH_CLIENT_ID or pass client id as an argument");

    // Create the builder!
    let mut builder = DeviceUserTokenBuilder::new(client_id, Default::default());

    // Start the device code flow. This will return a code that the user must enter on Twitch
    let code = builder.start(&reqwest).await?;

    println!("Please go to {0}", code.verification_uri);
    println!(
        "Waiting for user to authorize, time left: {0}",
        code.expires_in
    );

    // Finish the auth with wait_for_code, this will return a token if the user has authorized the app
    let mut token = builder.wait_for_code(&reqwest, tokio::time::sleep).await?;

    println!("token: {:?}\nTrying to refresh the token", token);
    // we can also refresh this token, even without a client secret
    // if the application was created as a public client type in the twitch dashboard this will work,
    // if the application is a confidential client type, this refresh will fail because it needs the client secret.
    token.refresh_token(&reqwest).await?;
    println!("refreshed token: {:?}", token);
    Ok(())
}

fn get_env_or_arg(env: &str, args: &mut impl Iterator<Item = String>) -> Option<String> {
    std::env::var(env).ok().or_else(|| args.next())
}

    Steps: Create DeviceUserTokenBuilder with Client ID and scopes. Call start for user code and verification URI. Display URI/code for Twitch authorization. Poll with wait_for_code for UserToken.

    Scopes: user:read:chat, user:write:chat, chat:read, chat:edit, moderator:manage:announcements, moderator:read:bans.

    LiveNAC: ui.rs spawns a task in start_login, updating UI from PendingAuth to LoggedIn.

    Docs: https://docs.rs/twitch_oauth2/0.15.2/twitch_oauth2/


All twitch_oauth2 Scopes

The twitch_oauth2::Scope enum provides a comprehensive list of all available scopes. Here is a breakdown of some of the key categories:

    Analytics: AnalyticsReadExtensions, AnalyticsReadGames

    Bits: BitsRead

    Channel Management: ChannelBot, ChannelEditCommercial, ChannelManageAds, ChannelManageBroadcast, ChannelManageExtensions, ChannelManageGuestStar, ChannelManageModerators, ChannelManagePolls, ChannelManagePredictions, ChannelManageRaids, ChannelManageRedemptions, ChannelManageSchedule, ChannelManageVideos, ChannelManageVips

    Channel Reading: ChannelReadAds, ChannelReadCharity, ChannelReadEditors, ChannelReadGoals, ChannelReadGuestStar, ChannelReadHypeTrain, ChannelReadPolls, ChannelReadPredictions, ChannelReadRedemptions, ChannelReadStreamKey, ChannelReadSubscriptions, ChannelReadVips

    Chat: ChatEdit, ChatRead, UserReadChat, UserWriteChat

    Moderation: ModerationRead, ModeratorManageAnnouncements, ModeratorManageAutoMod, ModeratorManageAutomodSettings, ModeratorManageBannedUsers, ModeratorManageBlockedTerms, ModeratorManageChatMessages, ModeratorManageChatSettings, ModeratorManageGuestStar, ModeratorManageShieldMode, ModeratorManageShoutouts, ModeratorManageUnbanRequests, ModeratorManageWarnings

    Moderation Reading: ModeratorReadAutomodSettings, ModeratorReadBannedUsers, ModeratorReadBlockedTerms, ModeratorReadChatMessages, ModeratorReadChatSettings, ModeratorReadChatters, ModeratorReadFollowers, ModeratorReadGuestStar, ModeratorReadModerators, ModeratorReadShieldMode, ModeratorReadShoutouts, ModeratorReadSuspiciousUsers, ModeratorReadUnbanRequests, ModeratorReadVips, ModeratorReadWarnings

    User Management: UserBot, UserEdit, UserEditBroadcast, UserManageBlockedUsers, UserManageChatColor, UserManageWhispers

    User Reading: UserReadBlockedUsers, UserReadBroadcast, UserReadChat, UserReadEmail, UserReadEmotes, UserReadFollows, UserReadModeratedChannels, UserReadSubscriptions, UserReadWhispers

    Whispers: WhispersEdit, WhispersRead

The eventsub Module: Full Reference

Modules

    automod: Automod related events

    channel: Subscription types regarding channels

    conduit: Subscription types regarding conduits

    event: EventSub events and their types

    stream: Subscription types regarding streams

    user: Subscription types regarding users

Structs

    Conduit: General information about a Conduit

    ConduitTransport: Conduit transport

    ConduitTransportResponse: Conduit transport

    EventSubSubscription: General information about an EventSub subscription

    EventSubscriptionInformation: Metadata about the subscription

    Payload: Notification received

    Shard: General information about a Shard

    ShardError: A structured error that occurred with a shard

    ShardResponse: A shard when described by Twitch

    VerificationRequest: Verification Request

    WebhookTransport: Webhook transport

    WebhookTransportResponse: Webhook transport

    WebsocketTransport: Websocket transport

    WebsocketTransportResponse: Websocket transport

Enums

    Event: A notification with an event payload. Enumerates all possible Payloads

    EventType: Event Types

    Message: Subscription message/payload. Received on events and other messages

    PayloadParseError: Errors that can happen when parsing payload

    ShardStatus: The shard status

    Status: Subscription request status

    Transport: Transport setting for event notification

    TransportMethod: Transport method

    TransportResponse: Transport response on event notification

Traits

    EventSubscription: An EventSub subscription


Sending Messages (Helix API)

Uses SendChatMessageRequest from twitch_api::helix, requiring user:write:chat.

use twitch_api::helix::chat::send_chat_message::{SendChatMessageBody, SendChatMessageRequest};
use twitch_types::{UserId, UserIdRef};
use twitch_oauth2::UserToken;

async fn send_chat_message(
    &self,
    broadcaster_id: &UserIdRef,
    sender_id: &UserIdRef,
    message: &str,
    token: &UserToken,
) -> Result<(), eyre::Report> {
    // The request object contains the broadcaster and sender IDs.
    let request = SendChatMessageRequest::new(broadcaster_id, sender_id);
    // The body object contains the message text.
    let body = SendChatMessageBody::new(message.to_string());
    
    // The req_post method expects a request, a body, and the token.
    let response = self.helix_client.req_post(request, body, token).await?;
    
    if let Some(data) = response.data.first() {
        if data.is_sent {
            tracing::info!("Message sent: {}", data.message_id);
        } else if let Some(reason) = &data.drop_reason {
            tracing::error!("Message dropped: {} - {}", reason.code, reason.message);
        }
    }
    
    Ok(())
}

    Details: Max Length: 500 characters; emotes by name (e.g., "Kappa"). Replies: Set reply_parent_message_id.

    Endpoint: POST https://api.twitch.tv/helix/chat/messages

    Docs: https://dev.twitch.tv/docs/api/reference/#send-chat-message

Sending Announcements (Helix API)

Uses SendChatAnnouncementRequest from twitch_api::helix, requiring moderator:manage:announcements.

use twitch_api::helix::chat::{SendChatAnnouncementRequest, ChatColor};
use twitch_types::UserId;
use twitch_oauth2::UserToken;

async fn send_chat_announcement(
    &self,
    broadcaster_id: &UserId,
    moderator_id: &UserId,
    message: &str,
    color: ChatColor,
    token: &UserToken,
) -> Result<(), eyre::Report> {
    let request = SendChatAnnouncementRequest {
        broadcaster_id: broadcaster_id.clone(),
        moderator_id: moderator_id.clone(),
        message: message.to_string(),
        color: Some(color), // e.g., ChatColor::Purple
    };
    self.helix_client.req_post(request, (), token).await?;
    tracing::info!("Announcement sent: {}", message);
    Ok(())
}

    Details: Max Length: 500 characters. Color: blue, green, orange, purple, primary (default: channel’s accent color). Rate Limit: One every 2 seconds.

    Endpoint: POST https://api.twitch.tv/helix/chat/announcements

    Docs: https://dev.twitch.tv/docs/api/reference/#send-chat-announcement

Checking Bans (Helix API)

Uses GetBannedUsersRequest from twitch_api::helix, requiring moderator:read:bans.

use twitch_api::helix::moderation::GetBannedUsersRequest;
use twitch_types::UserId;
use twitch_oauth2::UserToken;

async fn get_banned_users(
    &self,
    broadcaster_id: &UserId,
    moderator_id: &UserId,
    token: &UserToken,
) -> Result<Vec<String>, eyre::Report> {
    let request = GetBannedUsersRequest::new(broadcaster_id.clone(), moderator_id.clone());
    let response = self.helix_client.req_get(request, token).await?;
    let banned_users = response.data.into_iter().map(|user| user.user_login).collect();
    Ok(banned_users)
}

    Details: Returns list of banned users.

    https://docs.rs/twitch_api/0.7.2/twitch_api/helix/moderation/get_banned_users/index.html

    Endpoint: GET https://api.twitch.tv/helix/moderation/banned

    Docs: https://dev.twitch.tv/docs/api/reference/#get-banned-users


Migrating from IRC

Twitch recommends Helix/EventSub APIs, supported by twitch_api.

    PRIVMSG: Send Chat Message API (user:write:chat) for sending, Channel Chat Message EventSub (user:read:chat) for receiving.

    PRIVMSG Tags: Badges, Emotes, Replies, Cheers.

    CLEARCHAT: Channel Chat Clear, Channel Chat Clear User Messages.

    CLEARMSG: Channel Chat Message Delete.

    ROOMSTATE: Channel Chat Settings Update (EventSub), Get Chat Settings (API).

    USERSTATE/GLOBALUSERSTATE: Get Users, Get User Emotes, Get User Chat Color.

    USERNOTICE: Channel Chat Notification (subs, gifts, raids).

    JOIN/PART/NAMES: Get Chatters API (moderator:read:chatters).

    PING/PONG: WebSocket ping-pong; clients respond to pings.

    RECONNECT: EventSub Reconnect message.

    No Equivalents: NOTICE (handled by API errors or Revocation messages).

Twitch API Crate

The twitch_api crate enables Helix/EventSub interactions.

    Modules: helix, eventsub, client, types.

    Features: helix, eventsub, reqwest, twitch_oauth2.

    Example:

use twitch_api::helix::HelixClient;
use twitch_oauth2::{AccessToken, UserToken};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let client: HelixClient<reqwest::Client> = HelixClient::default();
    let token = UserToken::from_token(&client, AccessToken::from("mytoken")).await?;
    println!("Channel: {:?}", client.get_channel_from_login("twitchdev", &token).await?);
    Ok(())
}

    Docs: https://docs.rs/twitch_api/latest/twitch_api/

Types (twitch_types)

Provides type-safe identifiers.

    Types: UserId, Nickname, MsgId, DisplayName.

    Docs: https://docs.rs/twitch_types/0.4.8/twitch_types/

Running the App

    Run cargo run.

    Enter Client ID, click "Login to Twitch".

    Authorize in browser with URI/code.

    Join channel by name, click "Join Channel".

    Send messages or announcements via UI.

Future Expansions

    Render emotes/badges using Get Global/Channel/User Emotes, Get Global/Channel Chat Badges.

    Support Channel Chat Notification, Channel Chat Settings Update.

    Handle reply threads with reply_parent_message_id.

    Add ban monitoring via Get Banned Users.

---------------------------------------------------------------------------------------------------------------------------------------------

twitch_api::helix::eventsub API Reference

Implemented Endpoints

    Conduits (6/6)

        HelixClient::get_conduits

        HelixClient::create_conduit

        HelixClient::update_conduit

        HelixClient::delete_conduit

        HelixClient::get_conduit_shards

        HelixClient::update_conduit_shards

    EventSub (3/3)

        HelixClient::create_eventsub_subscription

        HelixClient::delete_eventsub_subscription

        HelixClient::get_eventsub_subscriptions

Modules

    create_conduit: Creates a new conduit for your client.

    create_eventsub_subscription: Creates an EventSub subscription.

    delete_conduit: Deletes a specified conduit.

    delete_eventsub_subscription: Deletes an EventSub subscription.

    get_conduit_shards: Gets a list of all shards for a conduit.

    get_conduits: Gets the conduits for your client.

    get_eventsub_subscriptions: Gets a list of your EventSub subscriptions.

    update_conduit: Updates a conduit’s shard count.

    update_conduit_shards: Updates shard(s) for a conduit.

Structs

    ConduitShards: Return values for Get Conduit Shards.

    CreateConduitBody: Body parameters for Create Conduit.

    CreateConduitRequest: Query parameters for Create Conduit.

    CreateEventSubSubscription: Return values for Create EventSub Subscription.

    CreateEventSubSubscriptionBody: Body parameters for Create EventSub Subscription.

    CreateEventSubSubscriptionRequest: Query parameters for Create EventSub Subscription.

    DeleteConduitRequest: Query parameters for Delete Conduit.

    DeleteEventSubSubscriptionRequest: Query parameters for Delete EventSub Subscriptions.

    EventSubSubscriptions: Return values for Get EventSub Subscriptions.

    GetConduitShardsRequest: Query parameters for Get Conduit Shards.

    GetConduitsRequest: Query parameters for Get Conduits.

    GetEventSubSubscriptionsRequest: Query parameters for Get EventSub Subscriptions.

    UpdateConduitBody: Body parameters for Update Conduit.

    UpdateConduitRequest: Query parameters for Update Conduit.

    UpdateConduitShardsBody: Body parameters for Update Conduit Shards.

    UpdateConduitShardsRequest: Query parameters for Update Conduit Shards.

    UpdateConduitShardsResponse: The structured response for Update Conduit Shards.

Enums

    DeleteConduitResponse: Return values for Delete Conduit.

    DeleteEventSubSubscription: Return values for Delete EventSub Subscriptions.


---------------------------------------------------------------------------------------------------------------------------------------------

twitch_api::eventsub::channel API Reference

Modules

    ad_break: Ad break on channel has begun

    ban: A viewer is banned from the specified channel.

    bits: Bits are used in a channel

    channel_points_automatic_reward_redemption: A viewer has redeemed an automatic channel points reward in a specified channel.

    channel_points_custom_reward: Custom channel points rewards on specific channel has been changed, removed or updated.

    channel_points_custom_reward_redemption: A viewer has redeemed a custom channel points reward or a redemption of a channel points custom reward has been updated for the specified channel.

    charity_campaign: Poll on a specific channel has been begun, ended or progressed.

    chat: Chat events

    chat_settings: A broadcaster’s chat settings are updated

    cheer: A user cheers on the specified channel.

    follow: A specified channel receives a follow.

    goal: A broadcaster has started, progressed or ended a goal.

    guest_star_guestbeta: Events regarding guests of guest star sessions

    guest_star_sessionbeta: Events regarding guest star sessions

    guest_star_settingsbeta: Events regarding settings of guest star sessions

    hypetrain: A hype train has started, progressed or ended.

    moderate: a moderator performs a moderation action in a channel.

    moderator: A user’s moderator privileges on a specified channel are changed.

    poll: Poll on a specific channel has been begun, ended or progressed.

    prediction: Prediction on the specified channel begins, progresses, locks or ends.

    raid: A a broadcaster raids another broadcaster’s channel.

    shared_chat: Events related to shared chat

    shield_mode: Shield mode on the specified channel begins or ends.

    shoutout: Subscription for when a Shoutout has happened

    subscribe: A specified channel receives a subscriber. This does not include resubscribes.

    subscription: Subscription on a specified channel has changed

    suspicious_user: A user’s moderator privileges on a specified channel are changed.

    unban: A viewer is unbanned from the specified channel.

    unban_request: An unban request in a specified channel is changed.

    update: Channel has updated the category, title, mature flag, or broadcast language.

    vip: A user’s VIP status on a specified channel is changed.

    warning: Notifications for warnings in a channel.

Structs

    ChannelAdBreakBeginV1: channel.ad_break.begin: a user runs a midroll commercial break, either manually or automatically via ads manager.

    ChannelAdBreakBeginV1Payload: channel.ad_break.begin response payload.

    ChannelBanV1: channel.ban: a viewer is banned from the specified channel.

    ChannelBanV1Payload: channel.ban response payload.

    ChannelBitsUseV1: channel.bits.use: sends a notification whenever Bits are used on a channel

    ChannelBitsUseV1Payload: channel.bits.use response payload.

    ChannelCharityCampaignDonateV1: channel.charity_campaign.donate: a user donates to the broadcaster’s charity campaign.

    ChannelCharityCampaignDonateV1Payload: channel.charity_campaign.donate response payload.

    ChannelCharityCampaignProgressV1: channel.charity_campaign.progress: progress is made towards the campaign’s goal or when the broadcaster changes the fundraising goal.

    ChannelCharityCampaignProgressV1Payload: channel.charity_campaign.progress response payload.

    ChannelCharityCampaignStartV1: channel.charity_campaign.start: a broadcaster starts a charity campaign.

    ChannelCharityCampaignStartV1Payload: channel.charity_campaign.start response payload.

    ChannelCharityCampaignStopV1: channel.charity_campaign.stop: a broadcaster stops a charity campaign.

    ChannelCharityCampaignStopV1Payload: channel.charity_campaign.stop response payload.

    ChannelChatClearUserMessagesV1: channel.chat.clear_user_messages: a moderator or bot clears all messages for a specific user.

    ChannelChatClearUserMessagesV1Payload: channel.chat.clear_user_messages response payload.

    ChannelChatClearV1: channel.chat.clear: a moderator or bot clears all messages from the chat room.

    ChannelChatClearV1Payload: channel.chat.clear response payload.

    ChannelChatMessageDeleteV1: channel.chat.message_delete: a moderator removes a specific message.

    ChannelChatMessageDeleteV1Payload: channel.chat.message_delete response payload.

    ChannelChatMessageV1: channel.chat.message: a user sends a message to a specific chat room.

    ChannelChatMessageV1Payload: channel.chat.message response payload.

    ChannelChatNotificationV1: channel.chat.notification: an event that appears in chat occurs, such as someone subscribing to the channel or a subscription is gifted.

    ChannelChatNotificationV1Payload: channel.chat.notification response payload.

    ChannelChatSettingsUpdateV1: channel.chat_settings.update: a broadcaster’s chat settings are updated.

    ChannelChatSettingsUpdateV1Payload: channel.chat_settings.update response payload.

    ChannelChatUserMessageHoldV1: channel.chat.user_message_hold: a user’s message is caught by automod.

    ChannelChatUserMessageHoldV1Payload: channel.chat.user_message_hold response payload.

    ChannelChatUserMessageUpdateV1: channel.chat.user_message_update: a user’s message’s automod status is updated.

    ChannelChatUserMessageUpdateV1Payload: channel.chat.user_message_update response payload.

    ChannelCheerV1: channel.cheer: a user cheers on the specified channel.

    ChannelCheerV1Payload: channel.cheer response payload.

    ChannelFollowV1Deprecated: channel.follow v1: a specified channel receives a follow.

    ChannelFollowV2: channel.follow v2: a specified channel receives a follow.

    ChannelFollowV1Payload: channel.follow response payload.

    ChannelFollowV2Payload: channel.follow response payload.

    ChannelGoalBeginV1: channel.goal.begin: a specified broadcaster begins a goal.

    ChannelGoalBeginV1Payload: channel.goal.begin response payload.

    ChannelGoalEndV1: channel.goal.end: a specified broadcaster ends a goal.

    ChannelGoalEndV1Payload: channel.goal.end response payload.

    ChannelGoalProgressV1: channel.goal.progress: progress is made towards the specified broadcaster’s goal.

    ChannelGoalProgressV1Payload: channel.goal.progress response payload.

    ChannelGuestStarGuestUpdateBetabeta: channel.guest_star_guest.update: the host preferences for Guest Star have been updated.

    ChannelGuestStarGuestUpdateBetaPayloadbeta: channel.guest_star_guest.update response payload.

    ChannelGuestStarSessionBeginBetabeta: channel.guest_star_session.begin: the host begins a new Guest Star session.

    ChannelGuestStarSessionBeginBetaPayloadbeta: channel.guest_star_session.begin response payload.

    ChannelGuestStarSessionEndBetabeta: a running Guest Star session is ended by the host, or automatically by the system.

    ChannelGuestStarSessionEndBetaPayloadbeta: channel.guest_star_session.end response payload.

    ChannelGuestStarSettingsUpdateBetabeta: channel.guest_star_settings.update: the host preferences for Guest Star have been updated.

    ChannelGuestStarSettingsUpdateBetaPayloadbeta: channel.guest_star_settings.update response payload.

    ChannelHypeTrainBeginV1: channel.hype_train.begin: a hype train begins on the specified channel.

    ChannelHypeTrainBeginV1Payload: channel.hype_train.begin response payload.

    ChannelHypeTrainEndV1: channel.hype_train.end: a hype train ends on the specified channel.

    ChannelHypeTrainEndV1Payload: channel.hype_train.end response payload.

    ChannelHypeTrainProgressV1: channel.hype_train.progress: a hype train makes progress on the specified channel.

    ChannelHypeTrainProgressV1Payload: channel.hype_train.progress response payload.

    ChannelModerateV1: channel.moderate: a moderator performs a moderation action in a channel.

    ChannelModerateV2: channel.moderate: a moderator performs a moderation action in a channel.

    ChannelModerateV1Payload: channel.moderate response payload.

    ChannelModerateV2Payload: channel.moderate response payload.

    ChannelModeratorAddV1: channel.moderator.add: a user is given moderator privileges on a specified channel.

    ChannelModeratorAddV1Payload: channel.moderator.add response payload.

    ChannelModeratorRemoveV1: channel.moderator.remove: a user has moderator privileges removed on a specified channel.

    ChannelModeratorRemoveV1Payload: channel.moderator.remove response payload.

    ChannelPointsAutomaticRewardRedemptionAddV1: channel.channel_points_automatic_reward_redemption.add:a viewer has redeemed an automatic channel points reward on the specified channel.

    ChannelPointsAutomaticRewardRedemptionAddV1Payload: channel.channel_points_automatic_reward_redemption.add response payload.

    ChannelPointsCustomRewardAddV1: a custom channel points reward has been created for the specified channel.

    ChannelPointsCustomRewardAddV1Payload: channel.channel_points_custom_reward.add response payload.

    ChannelPointsCustomRewardRedemptionAddV1: channel.channel_points_custom_reward_redemption.add: a viewer has redeemed a custom channel points reward on the specified channel.

    ChannelPointsCustomRewardRedemptionAddV1Payload: channel.channel_points_custom_reward_redemption.add response payload.

    ChannelPointsCustomRewardRedemptionUpdateV1: channel.channel_points_custom_reward_redemption.update: a redemption of a channel points custom reward has been updated for the specified channel.

    ChannelPointsCustomRewardRedemptionUpdateV1Payload: channel.channel_points_custom_reward_redemption.update response payload.

    ChannelPointsCustomRewardRemoveV1: a custom channel points reward has been removed from the specified channel.

    ChannelPointsCustomRewardRemoveV1Payload: channel.channel_points_custom_reward.remove response payload.

    ChannelPointsCustomRewardUpdateV1: a custom channel points reward has been updated for the specified channel.

    ChannelPointsCustomRewardUpdateV1Payload: channel.channel_points_custom_reward.update response payload.

    ChannelPollBeginV1: channel.poll.begin: a poll begins on the specified channel.

    ChannelPollBeginV1Payload: channel.poll.begin response payload.

    ChannelPollEndV1: channel.poll.end: a poll ends on the specified channel.

    ChannelPollEndV1Payload: channel.poll.end response payload.

    ChannelPollProgressV1: channel.poll.progress: an user responds to a poll on the specified channel

    ChannelPollProgressV1Payload: channel.poll.progress response payload.

    ChannelPredictionBeginV1: channel.prediction.begin: a Prediction begins on the specified channel

    ChannelPredictionBeginV1Payload: channel.prediction.begin response payload.

    ChannelPredictionEndV1: a Prediction ends on the specified channel.

    ChannelPredictionEndV1Payload: channel.prediction.end response payload.

    ChannelPredictionLockV1: an user responds to a prediction on the specified channel

    ChannelPredictionLockV1Payload: channel.prediction.lock response payload.

    ChannelPredictionProgressV1: an user responds to a prediction on the specified channel

    ChannelPredictionProgressV1Payload: channel.prediction.progress response payload.

    ChannelRaidV1: a a broadcaster raids another broadcaster’s channel.

    ChannelRaidV1Payload: channel.raid response payload.

    ChannelSharedChatBeginV1: a channel becomes active in an active shared chat session.

    ChannelSharedChatBeginV1Payload: channel.shared_chat.begin response payload.

    ChannelSharedChatEndV1: a channel leaves a shared chat session or the session ends.

    ChannelSharedChatEndV1Payload: channel.shared_chat.end response payload.

    ChannelSharedChatUpdateV1: the active shared chat session the channel is in changed.

    ChannelSharedChatUpdateV1Payload: channel.shared_chat.update response payload.

    ChannelShieldModeBeginV1: an user responds to a prediction on the specified channel

    ChannelShieldModeBeginV1Payload: channel.shield_mode.begin response payload.

    ChannelShieldModeEndV1: an user responds to a prediction on the specified channel

    ChannelShieldModeEndV1Payload: channel.shield_mode.end response payload.

    ChannelShoutoutCreateV1: a Prediction begins on the specified channel

    ChannelShoutoutCreateV1Payload: channel.shoutout.create response payload.

    ChannelShoutoutReceiveV1: a Prediction begins on the specified channel

    ChannelShoutoutReceiveV1Payload: channel.shoutout.receive response payload.

    ChannelSubscribeV1: a specified channel receives a subscriber. This does not include resubscribes.

    ChannelSubscribeV1Payload: channel.subscribe response payload.

    ChannelSubscriptionEndV1: a subscription to the specified channel expires.

    ChannelSubscriptionEndV1Payload: channel.subscription.end response payload.

    ChannelSubscriptionGiftV1: a subscription to the specified channel expires.

    ChannelSubscriptionGiftV1Payload: channel.subscription.gift response payload.

    ChannelSubscriptionMessageV1: a subscription to the specified channel expires.

    ChannelSubscriptionMessageV1Payload: channel.subscription.message response payload.

    ChannelSuspiciousUserMessageV1: a chat message has been sent from a suspicious user.

    ChannelSuspiciousUserMessageV1Payload: channel.suspicious_user.message response payload.

    ChannelSuspiciousUserUpdateV1: a suspicious user has been updated.

    ChannelSuspiciousUserUpdateV1Payload: channel.suspicious_user.update response payload.

    ChannelUnbanRequestCreateV1: a user creates an unban request.

    ChannelUnbanRequestCreateV1Payload: channel.unban_request.create response payload.

    ChannelUnbanRequestResolveV1: an unban request has been resolved.

    ChannelUnbanRequestResolveV1Payload: channel.unban_request.resolve response payload.

    ChannelUnbanV1: a viewer is unbanned from the specified channel.

    ChannelUnbanV1Payload: channel.unban response payload.

    ChannelUpdateV1Deprecated: version 1 of channel.update subscription type sends notifications when a broadcaster updates the category, title, mature flag, or broadcast language for their channel.

    ChannelUpdateV2: channel.update subscription type sends notifications when a broadcaster updates the category, title, mature flag, or broadcast language for their channel.

    ChannelUpdateV1PayloadDeprecated: channel.update response payload.

    ChannelUpdateV2Payload: channel.update response payload.

    ChannelVipAddV1: a VIP is added to the channel.

    ChannelVipAddV1Payload: channel.vip.add response payload.

    ChannelVipRemoveV1: a user has vip privileges removed on a specified channel.

    ChannelVipRemoveV1Payload: channel.vip.remove response payload.

    ChannelWarningAcknowledgeV1: a warning is acknowledged by a user.

    ChannelWarningAcknowledgeV1Payload: channel.warning.acknowledge response payload.

    ChannelWarningSendV1: a warning is sent to a user.

    ChannelWarningSendV1Payload: channel.warning.send response payload.

---------------------------------------------------------------------------------------------------------------------------------------------
    

twitch_api::eventsub API Reference
Implemented Subscriptions

    automod.* (6/6)

        automod.message.hold (v1 & v2)

        automod.message.update (v1 & v2)

        automod.settings.update (v1)

        automod.terms.update (v1)

    channel.* (66/67)

        Includes events for ad_break, ban, bits, channel_points, charity_campaign, chat, chat_settings, cheer, follow, goal, guest_star, hype_train, moderate, moderator, poll, prediction, raid, shared_chat, shield_mode, shoutout, subscribe, unban, update, vip, and warning.

    conduit.* (1/1)

        conduit.shard.disabled (v1)

    drop.* (0/1)

        drop.entitlement.grant (v1)

    extension.* (0/1)

        extension.bits_transaction.create (v1)

    stream.* (2/2)

        stream.offline (v1)

        stream.online (v1)

    user.* (4/4)

        user.authorization.grant (v1)

        user.authorization.revoke (v1)

        user.update (v1)

        user.whisper.message (v1)

Modules

    automod: Automod related events.

    channel: Subscription types regarding channels.

    conduit: Subscription types regarding conduits.

    event: EventSub events and their types.

    stream: Subscription types regarding streams.

    user: Subscription types regarding users.

Structs

    Conduit: General information about a Conduit.

    ConduitTransport: Conduit transport.

    ConduitTransportResponse: Conduit transport.

    EventSubSubscription: General information about an EventSub subscription.

    EventSubscriptionInformation: Metadata about the subscription.

    Payload: Notification received.

    Shard: General information about a Shard.

    ShardError: A structured error that occurred with a shard.

    ShardResponse: A shard when described by Twitch.

    VerificationRequest: Verification Request.

    WebhookTransport: Webhook transport.

    WebhookTransportResponse: Webhook transport.

    WebsocketTransport: Websocket transport.

    WebsocketTransportResponse: Websocket transport.

Enums

    Event: A notification with an event payload. Enumerates all possible Payloads.

    EventType: Event Types.

    Message: Subscription message/payload. Received on events and other messages.

    PayloadParseError: Errors that can happen when parsing payload.

    ShardStatus: The shard status.

    Status: Subscription request status.

    Transport: Transport setting for event notification.

    TransportMethod: Transport method.

    TransportResponse: Transport response on event notification.

Traits

    EventSubscription: An EventSub subscription.

---------------------------------------------------------------------------------------------------------------------------------------------

twitch_oauth2 Crate Reference

This document outlines the key components available in the twitch_oauth2 crate for handling Twitch OAuth2 authentication.
Re-exports

    AccessToken: A newtype for String representing an access token.

    ClientId: A newtype for String representing a client ID.

    ClientSecret: A newtype for String representing a client secret.

    CsrfToken: A newtype for String representing a CSRF token.

    RefreshToken: A newtype for String representing a refresh token.

    url: The url crate re-exported for URL manipulation.

Modules

    client: Provides different HTTP clients.

    id: Representation of the OAuth2 flow at id.twitch.tv.

    scopes: Contains all possible Twitch OAuth2 scopes.

    tokens: Defines Twitch token types.

    types: Core types used in the OAuth2 flow.

Macros

    validator: A macro to check if a slice of scopes matches a predicate.

Structs

    AppAccessToken: An App Access Token from the OAuth client credentials flow.

    DeviceUserTokenBuilder: A builder for the OAuth device code flow.

    ImplicitUserTokenBuilder: A builder for the OAuth implicit code flow.

    UserToken: A user token from the OAuth implicit or authorization code flow.

    UserTokenBuilder: A builder for the OAuth authorization code flow.

    ValidatedToken: The token validation response from https://id.twitch.tv/oauth2/validate.

Enums

    RequestParseError: Errors that can occur when parsing OAuth2 responses.

    Scope: Enumeration of all possible Twitch scopes.

    Validator: A validator for checking if an array of scopes matches a predicate.

Statics

    AUTH_URL: The authorization URL (https://id.twitch.tv/oauth2/authorize).

    DEVICE_URL: The device URL (https://id.twitch.tv/oauth2/device).

    REVOKE_URL: The revocation URL (https://id.twitch.tv/oauth2/revoke).

    TOKEN_URL: The token URL (https://id.twitch.tv/oauth2/token).

    VALIDATE_URL: The validation URL (https://id.twitch.tv/oauth2/validate).

Traits

    TwitchToken: A trait for Twitch token types to get common fields and generalize over AppAccessToken and UserToken.

---------------------------------------------------------------------------------------------------------------------------------------------
    

References

    https://docs.rs/twitch_api/latest/twitch_api/

    https://docs.rs/twitch_types/0.4.8/twitch_types/

    https://docs.rs/twitch_oauth2/0.15.2/twitch_oauth2/

    https://github.com/twitch-rs/twitch_api

    https://github.com/twitch-rs/twitch_types

    https://github.com/twitch-rs/twitch_oauth2

    https://dev.twitch.tv/docs/api/reference/#send-chat-announcement

    https://dev.twitch.tv/docs/api/reference/#get-banned-users

    https://dev.twitch.tv/docs/api/reference/#create-eventsub-subscription

    https://dev.twitch.tv/docs/api/reference/#get-eventsub-subscriptions

