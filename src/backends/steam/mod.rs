
use std::sync::Arc;

use bevy::ecs::resource::Resource;
use parking_lot::RwLock;
use steamworks::{ChatEntryType, FriendFlags, LobbyChatMsg, LobbyChatUpdate, LobbyCreated, LobbyEnter, LobbyType, P2PSessionRequest, SResult, SendType};
use crate::util::Receiver;
use crate::backends::{ChatKind, IBackendEvents, LobbyState, LobbyVisibility, OnLobbyCreate, OnLobbyMessage};
use bevy::log;

pub mod friends;
pub use friends::*;

pub type UserId = steamworks::SteamId;
pub type LobbyId = steamworks::LobbyId;

#[derive(Resource)]
pub struct Backend {
    raw: steamworks::Client,
    lobby: Arc<RwLock<LobbyState>>,
    members: Vec<UserId>,
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

        let lobby = Arc::new(RwLock::new(LobbyState::None));
        let events = BackendEvents::new(channel_size);

        // Lobby create event callback
        let tx = events.on_lobby_create.tx();
        client.register_callback(move |ev: LobbyCreated| {
            tx.blocking_send(ev).expect("[E559] (steam event channel disconnected)")
        });

        // lobby join event callback
        let tx = events.on_lobby_join.tx();
        let lobby2 = lobby.clone();
        client.register_callback(move |ev: LobbyEnter| {
            *lobby2.write() = LobbyState::InLobby(ev.lobby);
            tx.blocking_send(ev).expect("[E560] (steam event channel disconnected)")
        });

        // lobby message event callback
        let tx = events.on_lobby_msg.tx();
        let client2 = client.clone();
        client.register_callback(move |ev: LobbyChatMsg| {
            // get the content by querying its chatid
            let mut buf = Vec::new();
            client2.matchmaking().get_lobby_chat_entry(ev.lobby, ev.chat_id, &mut buf);
            // send the fully constructed message as UTF-8. 
            tx.blocking_send(
                OnLobbyMessage {
                    content: String::from_utf8(buf).unwrap_or_else(|_| "Not Valid UTF8".into()),
                    user: ev.user,
                    kind: convert_chat_entry_type(ev.chat_entry_type),
                }
            ).expect("[E561] (steam event channel disconnected)");
        });

        // lobby change event
        let tx = events.on_lobby_change.tx();
        client.register_callback(move |ev: LobbyChatUpdate| {
            tx.blocking_send(ev).expect("[E562] (steam event channel disconnected)")
        });

        // auto-accept all connection requests.
        let client2 = client.clone();
        client.register_callback(move |ev: P2PSessionRequest| {
            client2.networking().accept_p2p_session(ev.remote);
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
        self.lobby.read().clone()
    }

    fn create_lobby(
        &self, 
        vis: LobbyVisibility, 
        max_members: u32,
    ) -> bool {
        {
            let mut state = self.lobby.write();
            if *state != LobbyState::None {
                return false;
            }
            *state = LobbyState::Creating;
        }   

        let ty = match vis {
            LobbyVisibility::Anyone => LobbyType::Public,
            LobbyVisibility::FriendsOnly => LobbyType::FriendsOnly,
            LobbyVisibility::InviteOnly => LobbyType::Private,
        };
        self.raw.matchmaking().create_lobby(ty, max_members, log_cb);
        true
    }

    fn encode_lobby_id(&self) -> Option<String> {
        if let LobbyState::InLobby(id) = *self.lobby.read() {
            Some(base62::encode(id.raw()))
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
        {
            let mut state = self.lobby.write();
            if *state != LobbyState::None {
                return false;
            }
            *state = LobbyState::Joining;
        }
        self.raw.matchmaking().join_lobby(lobby, move |_| {});
        true
    }

    fn exit_lobby(&self) -> bool {
        let mut state = self.lobby.write();
        if let LobbyState::InLobby(id) = *state {
            *state = LobbyState::None;
            self.events.on_lobby_exit.send(id);
            self.raw.matchmaking().leave_lobby(id);
            true
        } else {
            false
        }
    }

    fn lobby_members(&self) -> Vec<UserId> {
        if let LobbyState::InLobby(id) = *self.lobby.read() {
            self.raw.matchmaking().lobby_members(id)
        } else {
            Vec::new()
        }
    }

    fn send_lobby_message(&self, msg: &str) {
        if let LobbyState::InLobby(id) = *self.lobby.read() {
            if let Err(e) = self.raw.matchmaking().send_lobby_chat_message(id, msg.as_bytes()) {
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
        if let LobbyState::InLobby(id) = *self.lobby.read() {
            self.members = self.lobby_members();
        } else {
            self.members.clear();
        }
    }
}

pub struct BackendEvents {
    /// A response to backend.create_lobby 
    on_lobby_create: Receiver<LobbyCreated>,

    /// A response to backend.join_lobby
    on_lobby_join: Receiver<LobbyEnter>,

    /// Occurs when backend.exit_lobby is called.
    on_lobby_exit: Receiver<LobbyId>,

    /// Occurs when a lobby message is received. 
    on_lobby_msg: Receiver<OnLobbyMessage>,

    /// Occurs when the member list changes.
    on_lobby_change: Receiver<LobbyChatUpdate>,
}

impl BackendEvents {
    fn new(size: usize) -> Self {
        Self {
            on_lobby_create: Receiver::new(size),
            on_lobby_join: Receiver::new(size),
            on_lobby_exit: Receiver::new(size),
            on_lobby_msg: Receiver::new(size),
            on_lobby_change: Receiver::new(size),
        }
    }
}

impl IBackendEvents for BackendEvents {
    fn read_lobby_create(&mut self) -> impl Iterator<Item=super::OnLobbyCreate> {
        use super::OnLobbyCreate::*;
        self.on_lobby_create.iter().map(|ev| {
            match ev.result {
                2 => Failed,
                16 => TimedOut,
                25 => LimitExceeded,
                15 => AccessDenied,
                3 => Offline,
                1 => Success(ev.lobby),
                _ => unreachable!("[E304] Unexpected error code from lobby create: '{}'", ev.result),
            }
        })
    }

    fn read_lobby_join(&mut self) -> impl Iterator<Item=super::OnLobbyJoin> {
        use steamworks::ChatRoomEnterResponse::*;
        use super::OnLobbyJoin;
        self.on_lobby_join.iter().map(|ev| {
            match ev.chat_room_enter_response {
                Success => OnLobbyJoin::Success(ev.lobby),
                DoesntExist => OnLobbyJoin::DoesntExist,
                NotAllowed => OnLobbyJoin::NotAllowed,
                Full => OnLobbyJoin::Full,
                Error => OnLobbyJoin::Error,
                Banned => OnLobbyJoin::Banned,
                // ?? what does limited mean
                Limited => OnLobbyJoin::Error,
                ClanDisabled => OnLobbyJoin::ClanDisabled,
                CommunityBan => OnLobbyJoin::CommunityBan,
                MemberBlockedYou => OnLobbyJoin::MemberBlockedYou,
                YouBlockedMember => OnLobbyJoin::YouBlockedMember,
                RatelimitExceeded => OnLobbyJoin::RatelimitExceeded,
            }
        })
    }

    fn read_lobby_exit(&mut self) -> impl Iterator<Item=super::OnLobbyExit> {
        self.on_lobby_exit.iter().map(|lobby| super::OnLobbyExit { lobby })
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