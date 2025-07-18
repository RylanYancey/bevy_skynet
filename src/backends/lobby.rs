
use super::*;

#[derive(States, Clone, Eq, PartialEq, Debug, Hash)]
pub enum LobbyState {
    /// The user is actively in a lobby.
    InLobby(LobbyId),

    /// The user is not in a lobby and is not attempting to create or join one.
    None,

    /// A request to create a lobby has been sent by this user. 
    Creating,

    /// A request to join a lobby has been sent by this user.
    Joining,
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
    pub lobby: LobbyId,
}

/// Response returned when the user attempts to join a lobby.
#[derive(Event, Debug)]
pub enum OnLobbyJoin {
    Success(LobbyId),
    Error,
    Full,
    Banned,
    DoesntExist,
    NotAllowed,
    NotInvited,
    ClanDisabled,
    CommunityBan,
    MemberBlockedYou,
    YouBlockedMember,
    RatelimitExceeded,
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

/// A request to create a lobby returned a response.
#[derive(Event, Debug, Clone, Eq, PartialEq)]
pub enum OnLobbyCreate {
    /// Lobby successfully created
    Success(LobbyId),

    /// Failed with an unknown error.
    Failed,

    /// Request timed out. 
    TimedOut,

    /// You created lobbies too fast.
    LimitExceeded,

    /// You don't have permission to create lobbies.
    AccessDenied,

    /// The user does not have a connection.
    Offline,
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

