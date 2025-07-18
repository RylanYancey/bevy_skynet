

//! Traits for ensuring all backends have the same API. 
//! 
//! A backend module MUST export publicly:
//!  - A "Backend" struct that implements "IBackend" and "Resource"
//!  - A "BackendEvents" struct that implements "IBackendEvents"
//!  - A "Friend" struct that implements "IFriend"
//!  - A "LobbyId" struct that implements Clone + Eq + PartialEq + Hash + Debug
//!  - A "UserId" struct

use bevy::prelude::*;
use bevy::log;

#[cfg(feature = "steam")]
pub mod steam;
#[cfg(feature = "steam")]
pub use steam::*;

pub mod lobby;
pub use lobby::*;

use crate::context::NetContext;
use crate::SkynetConfig;

/// Trait for ensuring uniformity across multiple backends. 
pub trait IBackend: Resource {
    fn from_config(config: &SkynetConfig) -> Self;

    /// The ID of the user. 
    fn user_id(&self) -> UserId;

    /// The name of the User. 
    fn user_name(&self) -> String;

    /// The user's configured UI language.
    fn preferred_ui_language(&self) -> Option<String>;

    /// Get a handle to the user's friend list.
    fn friends(&self) -> Vec<Friend>;

    /// Get the current state of the lobby.
    fn lobby_state(&self) -> LobbyState;

    /// Send a lobby create request. When a response is
    /// received, a OnLobbyCreate event will be dispatched.
    /// 
    /// If the user is already connected to a lobby or is joining/creating one,
    /// this function will return "false".
    fn create_lobby(&self, vis: LobbyVisibility, max_members: u32) -> bool;

    /// Convert the LobbyId of the lobby you're currently in to Base62. 
    /// Returns "None" if you're not currently in a lobby.
    fn encode_lobby_id(&self) -> Option<String>;

    /// Convert a LobbyId from base62 back to a u64. 
    /// Returns 0 and logs if the conversion fails. 
    fn decode_lobby_id(&self, id: String) -> LobbyId;

    /// Send a lobby join request. When a response is received, an OnLobbyEnter
    /// event will be dispatched. 
    /// 
    /// If the user is already connected to a lobby or is joining/creating one,
    /// this function will return "false".
    fn join_lobby(&self, lobby: LobbyId) -> bool;

    /// Send a lobby leave request for the current lobby. Dispatches an OnLobbyExit event.
    /// 
    /// If the user is not already connected to a lobby, "false" is returned.
    fn exit_lobby(&self) -> bool;

    /// Get the ids of other members in the lobby.
    fn lobby_members(&self) -> Vec<UserId>;

    /// Send a message to other players in the lobby.
    /// May log an error if the message is too small or too large.
    fn send_lobby_message(&self, msg: &str);

    /// Send a packet to the specified user. 
    /// The length of the data must be less than 1200. 
    /// Not intended for end-user use. 
    fn send_packet(&self, to: UserId, data: &[u8]);

    /// Broadcast a packet to all connected users in the lobby.
    /// The length of the data must be less than 1200.
    /// Not intended for end-user use.
    fn broadcast_packet(&self, data: &[u8]);

    /// Receive the next available packet. 
    /// Returns the id of the sender and the number of bytes written.
    /// Always sends in the highest reliability mode available. 
    /// 
    /// Not intended for end-user use. 
    fn recv_packet(&self, buf: &mut [u8]) -> Option<(UserId, usize)>;

    /// Get a reader over the Backend Events
    /// Not intended for end-user use. 
    fn events(&mut self) -> &mut BackendEvents;

    /// Ran once per frame. 
    /// Not intended for end-user use. 
    fn tick(&mut self);
}

/// Interface for reading and querying the state of a single friend.
pub trait IFriend {
    /// The name of the friend.
    /// If the friend is using a nickname, it is returned.
    /// Otherwise, the friend's account name is returned. 
    fn name(&self) -> String;
}

/// Interface for consuming events produced by the backend. 
/// This is not intended to be read directly by the user. 
pub trait IBackendEvents {
    /// Read the OnLobbyCreate events
    fn read_lobby_create(&mut self) -> impl Iterator<Item=OnLobbyCreate>;

    /// Read the OnLobbyJoin events
    fn read_lobby_join(&mut self) -> impl Iterator<Item=OnLobbyJoin>;

    /// Read the OnLobbyExit events
    fn read_lobby_exit(&mut self) -> impl Iterator<Item=OnLobbyExit>;

    /// Read the OnLobbyMsg events
    fn read_lobby_msg(&mut self) -> impl Iterator<Item=OnLobbyMessage>;

    /// Read the lobby message events
    fn read_lobby_change(&mut self) -> impl Iterator<Item=OnLobbyChange>;
}

/// Convert steamwork events to bevy events
pub fn read_backend_events(
    curr_state: Res<State<LobbyState>>,
    mut next_state: ResMut<NextState<LobbyState>>,
    mut backend: ResMut<Backend>,
    mut on_lobby_create: EventWriter<OnLobbyCreate>,
    mut on_lobby_join: EventWriter<OnLobbyJoin>,
    mut on_lobby_exit: EventWriter<OnLobbyExit>,
    mut on_lobby_msg: EventWriter<OnLobbyMessage>,
    mut on_lobby_change: EventWriter<OnLobbyChange>,
) {
    on_lobby_create.write_batch(backend.events().read_lobby_create());
    on_lobby_join.write_batch(backend.events().read_lobby_join());
    on_lobby_exit.write_batch(backend.events().read_lobby_exit());
    on_lobby_msg.write_batch(backend.events().read_lobby_msg());
    on_lobby_change.write_batch(backend.events().read_lobby_change());

    let curr = backend.lobby_state();
    if curr != *curr_state.get() {
        next_state.set(curr)
    }
}

/// Receive available packets and send them to the ECS for receipt. 
pub fn recv_incoming_packets(
    context: Res<NetContext>,
    backend: Res<Backend>,
) {
    let registry = context.messages.clone();
    let mut buf = Vec::with_capacity(1200);
    while let Some((user_id, len)) = backend.recv_packet(&mut buf) {
        if len < 8 {
            log::warn!("P2P Backend Received a packet that was too small and was discarded (len: '{}')", buf.len());
        } else {
            let msg_id = u64::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]);
            registry.send(msg_id, &buf[8..], user_id);
        }
        buf.clear();
    }
}

/// Execute the backend's tick 
pub fn tick_backend(mut backend: ResMut<Backend>) {
    backend.tick();
}