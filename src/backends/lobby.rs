
use super::*;

#[derive(States, Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum LobbyState {
    /// The user is actively in a lobby.
    InLobby,

    /// The user is not in a lobby and is not attempting to create or join one.
    #[default]
    None,

    /// The user is attempting to join or create a lobby.
    Joining,
}

/// Whether this user is the host. 
#[derive(States, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum IsLobbyHost {
    True,

    #[default]
    False,
}

/// Only exists when a player is in a lobby.
#[derive(Clone, Resource)]
pub struct CurrentLobby {
    /// The unique ID associated with this lobby.
    pub id: LobbyId,

    /// The join policy of the lobby.
    pub vis: LobbyVisibility,

    /// Whether the user is the host. 
    pub is_host: bool,

    /// The max number of members allowed in the lobby.
    pub max_members: u32,

    /// The invite code that can be sent to others so they can join.
    /// This is the LobbyId base62 encoded. 
    pub invite_code: String,

    /// UserIds of other members in the lobby. 
    /// This does not include this client. 
    pub others: Vec<UserId>,
}

impl Default for CurrentLobby {
    fn default() -> Self {
        Self {
            id: LobbyId::from_raw(0),
            vis: LobbyVisibility::Anyone,
            max_members: 4,
            is_host: false,
            invite_code: String::new(),
            others: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum LobbyVisibility {
    Anyone,
    FriendsOnly,
    InviteOnly,
}

/// Occurs when the user exits a lobby.
#[derive(Event, Debug)]
pub struct OnLobbyExit {
    pub id: LobbyId,
}

/// Response returned when the user attempts to join a lobby.
#[derive(Event, Debug)]
pub struct OnLobbyJoin {
    pub id: LobbyId,
}

/// A chat message was received from the current lobby.
#[derive(Event, Debug)]
pub struct OnLobbyMessage {
    /// The content of the message.
    pub content: String,

    /// The user who sent the message.
    pub user: UserId,

    /// The purpose of the message.
    pub kind: ChatKind,
}

/// Copied directly from steamworks sdk
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ChatKind {
    Invalid,
    ChatMsg,
    Typing,
    InviteGame,
    Emote,
    LeftConversation,
    Entered,
    WasKicked,
    WasBanned,
    Disconnected,
    HistoricalChat,
    LinkBlocked,
}

/// A member list of a lobby the user is connected to change.  
#[derive(Event, Debug, Clone)]
pub enum OnLobbyChange {
    /// A user entered the lobby.
    Joined(UserId),

    /// A user exited the lobby. 
    Exited(UserId),

    /// A user was kicked from the lobby.
    Kicked {
        /// The user that was kicked
        target: UserId,

        /// The user that executed the kick.
        executor: UserId,
    },

    /// A user was banned from the lobby.
    Banned {
        /// The user that was banned
        target: UserId,

        /// The user that executed the ban.
        executor: UserId,
    },
}

#[derive(Event, Debug, Clone)]
pub struct LobbyConnectError {
    pub id: LobbyId,
    pub kind: LobbyErrorKind,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LobbyErrorKind {
    /// Request timed out. 
    TimedOut,

    /// Too many create/join requests.
    TooFast,

    /// You are not allowed to join this room,
    /// OR not allowed to create lobbies. 
    AccessDenied,

    /// An Invite is required to join the lobby.
    InviteRequired,

    /// Unable to connect to steam.
    Offline,

    /// The lobby was not found.
    NotFound,

    /// The lobby was full.
    Full,

    /// You are banned from this lobby.
    Banned,

    /// You are blocked by a member in the lobby, or
    /// you are blocking a member in the lobby.
    Blocked,

    /// An unknown error code was returned.
    Unknown,
}

impl std::fmt::Display for LobbyErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use LobbyErrorKind::*;
        f.write_str(match *self {
            TimedOut => "Timed Out",
            AccessDenied => "Access Denied",
            InviteRequired => "Invitation Required",
            Offline => "Offline",
            NotFound => "Not Found",
            Full => "Full",
            Banned => "Banned",
            Blocked => "Blocking or Blocked by User",
            Unknown => "Unknown Error",
            TooFast => "Rate Limit Exceeded"
        })
    }
}

#[cfg(feature = "steam")]
impl TryFrom<steamworks::ChatRoomEnterResponse> for LobbyErrorKind {
    type Error = ();
    fn try_from(value: steamworks::ChatRoomEnterResponse) -> Result<Self, ()> {
        use steamworks::ChatRoomEnterResponse::*;
        Ok(match value {
            Success => return Err(()),
            DoesntExist => Self::NotFound,
            NotAllowed => Self::AccessDenied,
            Full => Self::Full,
            Error => Self::Unknown,
            Banned => Self::Banned,
            Limited => Self::TooFast,
            ClanDisabled => Self::Banned,
            CommunityBan => Self::Banned,
            MemberBlockedYou => Self::Blocked,
            YouBlockedMember => Self::Blocked,
            RatelimitExceeded => Self::TooFast,
        })
    }
}

#[cfg(feature = "steam")]
impl TryFrom<steamworks::LobbyCreated> for LobbyErrorKind {
    type Error = ();
    fn try_from(value: steamworks::LobbyCreated) -> Result<Self, Self::Error> {
        use LobbyErrorKind::*;
        Ok(match value.result {
            1 => return Err(()),
            16 => TimedOut,
            25 => TooFast,
            15 => AccessDenied,
            3 => Offline,
            _ => Unknown,
        })
    }
}
