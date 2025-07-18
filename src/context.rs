
use bevy::prelude::*;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use crate::{backends::UserId, comms::DynamicTx, SkynetConfig};
use bevy::log;

#[derive(Resource)]
pub struct NetContext {
    pub messages: Arc<MessageRegistry>,
    pub config: SkynetConfig,
}

impl NetContext {
    pub fn new(config: SkynetConfig) -> Self {
        Self {
            messages: Arc::new(MessageRegistry::default()),
            config,
        }
    }

    pub fn config(&self) -> &SkynetConfig {
        &self.config
    }
}

pub struct MessageType {
    /// The fully-qualified path of the type and its identifier.
    pub(crate) name: &'static str,

    /// The XXH64 hash of the name.
    pub(crate) id: u64,

    /// Transmitter that deserializes and sends messages to the incoming rx.
    pub(crate) tx: Box<dyn DynamicTx>,
}

#[derive(Default)]
pub struct MessageRegistry {
    registry: RwLock<BTreeMap<u64, Arc<MessageType>>>,
}

impl MessageRegistry {
    pub fn insert(&self, msg: Arc<MessageType>) {
        let name = msg.name;
        if let Some(existing) = self.registry.write().insert(msg.id, msg) {
            panic!("Failed to add Network Message '{}' because it has the same ID as the existing Message '{}'.", name, existing.name);
        }
    }

    pub fn send(&self, msg_id: u64, payload: &[u8], sender: UserId) {
        match self.registry.read().get(&msg_id) {
            None => log::error!("A message was received, but it was not registered in the NetContext."),
            Some(ty) => {
                if let Err(e) = ty.tx.send(payload, sender) {
                    log::error!("A message failed to deserialize with error: '{e}'.");
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Message<T> {
    pub sender: UserId,
    pub payload: T,
}