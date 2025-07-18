
use std::sync::Arc;

use bevy::ecs::resource::Resource;
use bevy::utils::default;
use parking_lot::RwLock;
use steamworks::{ChatEntryType, FriendFlags, GameLobbyJoinRequested, LobbyChatMsg, LobbyChatUpdate, LobbyCreated, LobbyEnter, LobbyType, P2PSessionRequest, SResult, SendType};
use crate::prelude::{OnLobbyExit, OnLobbyJoin};
use crate::util::Receiver;
use crate::backends::{ChatKind, CurrentLobby, IBackendEvents, LobbyErrorKind, LobbyState, LobbyVisibility, LobbyConnectError, OnLobbyMessage};
use bevy::log;

pub mod friends;
pub use friends::*;

pub type UserId = steamworks::SteamId;
pub type LobbyId = steamworks::LobbyId;

#[derive(Resource)]
pub struct Backend {
    /// Raw handle to steamworks client.
    raw: steamworks::Client,

    /// Lobby state and type information
    lobby: Arc<RwLock<LobbyData>>,

    /// Re-computed each tick so we don't have to call lobby_members() on each broadcasting send.
    members: Vec<UserId>,

    /// Event receivers
    events: BackendEvents,
}

impl Backend {
    /// Initialize the Steamworks backend.
    pub fn new(app_id: u32, channel_size: usize) -> Self {
        let client = match steamworks::Client::init_app(app_id) {
            Ok(client) => client,
            Err(e) => {
                panic!("An error occured while initializing Steamworks Client backend: '{e}'.");
            }
        };

        let lobby = Arc::new(RwLock::new(LobbyData::default()));
        let events = BackendEvents::new(channel_size);

        // Lobby create event callback
        let lobby2 = lobby.clone();
        let tx = events.on_lobby_error.tx();
        client.register_callback(move |ev: LobbyCreated| {
            let id = ev.lobby;
            // check if this is an error code 
            if let Ok(kind) = LobbyErrorKind::try_from(ev) {
                // creation failed, lobby state is now None. 
                lobby2.write().state = LobbyState::None;

                // Dispatch the error for processing. 
                if let Err(_) = tx.try_send(LobbyConnectError { id, kind }) {
                    log::warn!("[E559] A LobbyError was received, but its event receiver is full.")
                }
            }
        });

        // lobby join event callback
        let join_tx = events.on_lobby_join.tx();
        let err_tx = events.on_lobby_error.tx();
        let lobby2 = lobby.clone();
        let client2 = client.clone();
        client.register_callback(move |ev: LobbyEnter| {
            match LobbyErrorKind::try_from(ev.chat_room_enter_response) {
                // LobbyError occured while joining
                Ok(kind) => {
                    // kick from join queue
                    lobby2.write().state = LobbyState::None;

                    // send error event
                    if let Err(_) = err_tx.try_send(LobbyConnectError { id: ev.lobby, kind }) {
                        log::error!("[E558] A LobbyError was received, but its event receiver is full.");
                    }
                }
                // No error, join was successful
                Err(_) => {
                    // send error event
                    if let Err(_) = join_tx.try_send(OnLobbyJoin { id: ev.lobby }) {
                        log::error!("[E557] A LobbyError was received, but its event receiver is full.");
                    }
                    // update lobby state
                    let mut lobby = lobby2.write();
                    lobby.state = LobbyState::InLobby;
                    lobby.curr.id = ev.lobby;
                    lobby.curr.invite_code = base62::encode(ev.lobby.raw());
                    lobby.curr.max_members = client2.matchmaking().lobby_member_limit(ev.lobby).unwrap_or(4) as u32;
                    lobby.curr.others = client2.matchmaking().lobby_members(ev.lobby);
                }
            }
        });

        // lobby message event callback
        let tx = events.on_lobby_msg.tx();
        let client2 = client.clone();
        client.register_callback(move |ev: LobbyChatMsg| {
            // get the content by querying its chatid
            let mut buf = Vec::new();
            client2.matchmaking().get_lobby_chat_entry(ev.lobby, ev.chat_id, &mut buf);

            // Extract the content and check if it's valid UTF-8. 
            let content = String::from_utf8(buf).unwrap_or_else(|_| {
                log::warn!("Received a Chat Message from Steam that was not valid UTF-8.");
                "!! ERROR !! Not Valid UTF-8".into()
            });

            // get the type of message
            let kind = convert_chat_entry_type(ev.chat_entry_type);

            // send the event
            if let Err(_) = tx.try_send(OnLobbyMessage { content, user: ev.user, kind }) {
                log::error!("[E557] A LobbyMessage was received, but its event receiver is full.");
            }
        });

        // lobby change event
        let tx = events.on_lobby_change.tx();
        client.register_callback(move |ev: LobbyChatUpdate| {
            if let Err(_) = tx.try_send(ev) {
                log::error!("[E556] A LobbyChange was received, but its event receiver is full.");
            }
        });

        // auto-accept all connection requests.
        let client2 = client.clone();
        client.register_callback(move |ev: P2PSessionRequest| {
            log::trace!("Accepted P2P Session Request from UserID: '{:?}'", ev.remote);
            client2.networking().accept_p2p_session(ev.remote);
        });

        // Auto accept attempts by the user to join a lobby by clicking "Join Game" or "Accept Invite"
        // within the steam menu. 
        let client2 = client.clone();
        let lobby2 = lobby.clone();
        let exit_tx = events.on_lobby_exit.tx();
        client.register_callback(move |ev: GameLobbyJoinRequested| {
            let mut lobby = lobby2.write();
            // do nothing if we are already joining 
            if lobby.state != LobbyState::Joining {
                // send lobby exit event if already in lobby
                if lobby.state == LobbyState::InLobby {
                    client2.matchmaking().leave_lobby(lobby.curr.id);
                    if let Err(_) = exit_tx.try_send(OnLobbyExit { id: lobby.curr.id }) {
                        log::error!("[E555] A LobbyExit occurred, but its event receiver was full.")
                    }
                }

                // send lobby join request
                client2.matchmaking().join_lobby(ev.lobby_steam_id, move |_| {});
                lobby.state = LobbyState::Joining;
                lobby.curr = CurrentLobby {
                    id: ev.lobby_steam_id,
                    is_host: false,
                    ..default()
                };
            }
        });

        Self {
            raw: client,
            lobby,
            members: Vec::new(),
            events
        }
    }
}

impl super::IBackend for Backend {
    fn from_config(config: &crate::SkynetConfig) -> Self {
        Self::new(config.steamworks.app_id, config.general.channel_size as usize)
    }

    fn user_id(&self) -> UserId {
        self.raw.user().steam_id()
    }

    fn user_name(&self) -> String {
        self.raw.friends().name()
    }

    fn name_of(&self, user: UserId) -> String {
        let friend = self.raw.friends().get_friend(user);
        friend.nick_name().unwrap_or_else(|| friend.name())
    }

    fn preferred_ui_language(&self) -> Option<String> {
        Some(self.raw.utils().ui_language())
    }

    fn friends(&self) -> Vec<Friend> {
        self.raw.friends()
            .get_friends(FriendFlags::IMMEDIATE)
            .into_iter()
            .map(|friend| Friend(friend))
            .collect::<Vec<_>>()
    }

    fn lobby_state(&self) -> LobbyState {
        self.lobby.read().state.clone()
    }

    fn current_lobby(&self) -> Option<CurrentLobby> {
        self.lobby.read().get_if_in_lobby().map(|mut curr| {
            curr.others = self.lobby_members();
            curr
        })
    }

    fn create_lobby(
        &self, 
        vis: LobbyVisibility, 
        max_members: u32,
    ) -> bool {
        let data = CurrentLobby {
            vis,
            max_members,
            is_host: true,
            ..default()
        };

        if let Some(_) = self.lobby.write().set_joining_if_none(data) {
            let ty = match vis {
                LobbyVisibility::Anyone => LobbyType::Public,
                LobbyVisibility::FriendsOnly => LobbyType::FriendsOnly,
                LobbyVisibility::InviteOnly => LobbyType::Private,
            };
            self.raw.matchmaking().create_lobby(ty, max_members, log_cb);
            true
        } else {
            false
        }
    }

    fn encode_lobby_id(&self) -> Option<String> {
        if let Some(curr) = self.lobby.read().get_if_in_lobby() {
            Some(base62::encode(curr.id.raw()))
        } else {
            None
        }
    }

    fn decode_lobby_id(&self, id: String) -> LobbyId {
        match base62::decode(id.as_bytes()) {
            Err(e) => {
                log::error!("Failed to decode lobby ID with error: '{e}'");
                LobbyId::from_raw(0)
            },
            Ok(n) => LobbyId::from_raw(n as u64),
        }
    }

    fn join_lobby(&self, lobby: LobbyId) -> bool {
        let data = CurrentLobby {
            id: lobby,
            is_host: false,
            ..default()
        };

        if let Some(_) = self.lobby.write().set_joining_if_none(data) {
            self.raw.matchmaking().join_lobby(lobby, move |_| {});
            true
        } else {
            false
        }
    }

    fn exit_lobby(&self) -> bool {
        if let Some(curr) = self.lobby.read().get_if_in_lobby() {
            self.events.on_lobby_exit.send(OnLobbyExit { id: curr.id });
            self.raw.matchmaking().leave_lobby(curr.id);
            self.lobby.write().state = LobbyState::None;
            true
        } else {
            false
        }
    }

    fn lobby_members(&self) -> Vec<UserId> {
        if let Some(curr) = self.lobby.read().get_if_in_lobby() {
            let mut members = self.raw.matchmaking().lobby_members(curr.id);
            let user = self.user_id();
            if let Some(i) = members.iter().position(|id| user == *id) {
                members.remove(i);
            }
            members
        } else {
            Vec::new()
        }
    }

    fn send_lobby_message(&self, msg: &str) {
        if let Some(curr) = self.lobby.read().get_if_in_lobby() {
            if let Err(e) = self.raw.matchmaking().send_lobby_chat_message(curr.id, msg.as_bytes()) {
                log::error!("Attempted to send a lobby chat message, but steam returned an error: '{e}'");
            }
        }
    }

    fn send_packet(&self, to: UserId, data: &[u8]) {
        self.raw.networking().send_p2p_packet(to, SendType::Reliable, data);
    }

    fn broadcast_packet(&self, data: &[u8]) {
        for member in &self.members {
            self.send_packet(*member, data);
        }
    }

    fn recv_packet(&self, buf: &mut [u8]) -> Option<(UserId, usize)> {
        self.raw.networking().read_p2p_packet(buf)
    }

    fn events(&mut self) -> &mut BackendEvents {
        &mut self.events
    }

    fn tick(&mut self) {
        if let Some(_) = self.lobby.read().get_if_in_lobby() {
            self.members = self.lobby_members();
        } else {
            self.members.clear();
        }
    }
}

pub struct BackendEvents {
    /// A response to backend.join_lobby
    on_lobby_join: Receiver<OnLobbyJoin>,

    /// Occurs when backend.exit_lobby is called.
    on_lobby_exit: Receiver<OnLobbyExit>,

    /// Occurs when a lobby message is received. 
    on_lobby_msg: Receiver<OnLobbyMessage>,

    /// Occurs when the member list changes.
    on_lobby_change: Receiver<LobbyChatUpdate>,

    /// Errors that can occur when joining or creating. 
    on_lobby_error: Receiver<LobbyConnectError>,
}

impl BackendEvents {
    fn new(size: usize) -> Self {
        Self {
            on_lobby_join: Receiver::new(size),
            on_lobby_exit: Receiver::new(size),
            on_lobby_msg: Receiver::new(size),
            on_lobby_change: Receiver::new(size),
            on_lobby_error: Receiver::new(size),
        }
    }
}

impl IBackendEvents for BackendEvents {
    fn read_lobby_join(&mut self) -> impl Iterator<Item=super::OnLobbyJoin> {
        self.on_lobby_join.iter()
    }

    fn read_lobby_exit(&mut self) -> impl Iterator<Item=super::OnLobbyExit> {
        self.on_lobby_exit.iter()
    }

    fn read_lobby_msg(&mut self) -> impl Iterator<Item=OnLobbyMessage> {
        self.on_lobby_msg.iter()
    }

    fn read_lobby_change(&mut self) -> impl Iterator<Item=super::OnLobbyChange> {
        use steamworks::ChatMemberStateChange::*;
        use super::OnLobbyChange;
        self.on_lobby_change.iter().map(|ev| {
            match ev.member_state_change {
                Entered => OnLobbyChange::Joined(ev.user_changed),
                Left => OnLobbyChange::Exited(ev.user_changed),
                Kicked => OnLobbyChange::Kicked {
                    target: ev.user_changed,
                    executor: ev.making_change,
                },
                Banned => OnLobbyChange::Banned {
                    target: ev.user_changed,
                    executor: ev.making_change,
                },
                Disconnected => OnLobbyChange::Exited(ev.user_changed),
            }
        })
    }

    fn read_lobby_connect_errors(&mut self) -> impl Iterator<Item=LobbyConnectError> {
        self.on_lobby_error.iter()
    }
}

fn log_cb<T>(res: SResult<T>) {
    if let Err(e) = res {
        log::error!("The Steamworks API emitted an Error: '{e}'")
    }
}

fn convert_chat_entry_type(entry: ChatEntryType) -> ChatKind {
    match entry {
        ChatEntryType::Invalid => ChatKind::Invalid,
        ChatEntryType::ChatMsg => ChatKind::ChatMsg,
        ChatEntryType::Typing => ChatKind::Typing,
        ChatEntryType::InviteGame => ChatKind::InviteGame,
        ChatEntryType::Emote => ChatKind::Emote,
        ChatEntryType::LeftConversation => ChatKind::LeftConversation,
        ChatEntryType::Entered => ChatKind::Entered,
        ChatEntryType::WasKicked => ChatKind::WasKicked,
        ChatEntryType::WasBanned => ChatKind::WasBanned,
        ChatEntryType::Disconnected => ChatKind::Disconnected,
        ChatEntryType::HistoricalChat => ChatKind::HistoricalChat,
        ChatEntryType::LinkBlocked => ChatKind::LinkBlocked,
    }
}

#[derive(Clone)]
struct LobbyData {
    state: LobbyState,
    curr: CurrentLobby,
}

impl Default for LobbyData {
    fn default() -> Self {
        Self {
            state: LobbyState::None,
            curr: CurrentLobby::default()
        }
    }
}

impl LobbyData {
    fn set_joining_if_none(
        &mut self, 
        data: CurrentLobby,
    ) -> Option<Self> {
        if self.state == LobbyState::None {
            self.curr = data;
            Some(self.clone())
        } else {
            None
        }
    }

    fn get_if_in_lobby(&self) -> Option<CurrentLobby> {
        if self.state == LobbyState::InLobby {
            Some(self.curr.clone())
        } else {
            None
        }
    }
}