use std::{collections::BTreeMap, io, marker::PhantomData, sync::Arc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::sync::mpsc;
use xxhash_rust::const_xxh64::xxh64;
use bevy::{log, prelude::*};
use crate::{comms::{IncomingRx, OutgoingTx}, context::{Message, MessageType, NetContext}};

pub mod prelude {
    pub type Client = crate::backends::Backend;

    pub use crate::{
        SkynetAppExt,
        SkynetConfig,
        SkynetPlugin,
        params::{NetReceiver, NetSender},
        backends::{
            Backend,
            OnLobbyChange,
            OnLobbyJoin,
            OnLobbyExit,
            LobbyConnectError,
            OnLobbyMessage,
            LobbyErrorKind,
            IsLobbyHost,
            LobbyVisibility,
            LobbyState,
            ChatKind,
            IBackend,
            IFriend,
            Friend,
            LobbyId,
            UserId,
        }
    };
}

pub mod backends;
pub mod context;
pub mod params;
pub mod comms;
pub mod util;

pub struct SkynetPlugin;

impl Plugin for SkynetPlugin {
    fn build(&self, app: &mut App) {
        use backends::*;
        let config = SkynetConfig::load_or_default();
    
        // Steamworks expects a steam_appid.txt to load, so we generate one from the config when in debug mode. 
        #[cfg(feature = "steam")]
        if cfg!(debug_assertions) {
            std::fs::write("steam_appid.txt", config.steamworks.app_id.to_string())
                .expect("Failed to write steam_appid.txt");
        }

        app
            .insert_resource(Backend::from_config(&config))
            .insert_resource(NetContext::new(config))
            .add_event::<OnLobbyJoin>()
            .add_event::<OnLobbyExit>()
            .add_event::<OnLobbyChange>()
            .add_event::<OnLobbyMessage>()
            .add_event::<OnLobbyChange>()
            .add_event::<LobbyConnectError>()
            .init_state::<LobbyState>()
            .init_state::<IsLobbyHost>()
            .add_systems(
                Last, (
                    backends::read_backend_events,
                    backends::recv_incoming_packets
                        .after(backends::read_backend_events)
                )
            )
        ;
    }
}

pub trait SkynetAppExt {
    fn add_message<T>(&mut self) -> &mut Self
    where
        T: TypePath + DeserializeOwned + Send + Sync;
}

impl SkynetAppExt for App {
    fn add_message<T>(&mut self) -> &mut Self
    where
        T: TypePath + DeserializeOwned + Send + Sync
    {
        const SEED: u64 = 0x9e3779b185ebca87;

        self.add_systems(PreStartup,
            move |ctx: Res<NetContext>, mut commands: Commands| {
                let (tx, rx) = mpsc::channel::<Message<T>>(ctx.config.general.channel_size as usize);
                let name = T::type_path();
                let msg = Arc::new(
                    MessageType {
                        name,
                        id: xxh64(name.as_bytes(), SEED),
                        tx: Box::new(comms::IncomingTx { tx })
                    }
                );
                ctx.messages.insert(msg.clone());
                commands.insert_resource(OutgoingTx::<T> { message: msg, _marker: PhantomData });
                commands.insert_resource(IncomingRx { rx, });
            }
        )
    }
}

#[derive(Deserialize, Default)]
pub struct SkynetConfig {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub steamworks: SteamworksConfig,
}

impl SkynetConfig {
    pub fn load_or_default() -> SkynetConfig {
        use std::path::Path;
        use std::fs;
        let path = Path::new("Skynet.toml");
        if path.exists() {
            match fs::read_to_string(path)
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
            {
                Some(config) => config,
                None => {
                    log::warn!("Failed to parse Skynet.toml, generating default config.");
                    SkynetConfig::default()
                }
            }
        } else {
            log::warn!("Skynet.toml not found, generating default config.");
            SkynetConfig::default()
        }
    }
}

#[derive(Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub channel_size: u32,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            channel_size: 64
        }
    }
}

#[derive(Deserialize)]
pub struct SteamworksConfig {
    #[serde(default)]
    pub app_id: u32,
}

impl Default for SteamworksConfig {
    fn default() -> Self {
        Self {
            app_id: 480
        }
    }
}