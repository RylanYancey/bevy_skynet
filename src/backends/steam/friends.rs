use steamworks::FriendFlags;

use crate::backends::IFriend;

pub struct Friend(pub steamworks::Friend);

impl IFriend for Friend {
    fn name(&self) -> String {
        self.0.nick_name().unwrap_or_else(|| self.0.name())
    }
}
