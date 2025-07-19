use tokio::sync::mpsc;
use std::{io, marker::PhantomData, sync::Arc};
use serde::de::DeserializeOwned;
use bevy::prelude::*;
use crate::{backends::UserId, context::{Message, MessageType}};
use bevy::log;

type CborError = ciborium::de::Error<io::Error>;

#[derive(Resource)]
pub struct IncomingRx<T> {
    pub(crate) rx: mpsc::Receiver<Message<T>>,
}

pub struct IncomingTx<T> {
    pub(crate) tx: mpsc::Sender<Message<T>>,
}

pub(crate) trait DynamicTx: Send + Sync {
    fn send(&self, payload: &[u8], sender: UserId) -> Result<(), CborError>;
}

impl<T> DynamicTx for IncomingTx<T>
where
    T: DeserializeOwned + Send + Sync + TypePath
{
    fn send(&self, payload: &[u8], sender: UserId) -> Result<(), CborError> {
        let cursor = io::Cursor::new(payload);
        let payload: T = ciborium::from_reader(cursor)?;
        if let Err(_) = self.tx.try_send(Message { sender, payload }) {
            log::error!("The message receiver channel for message '{}' is full.", T::type_path())
        }
        Ok(())
    }
}

#[derive(Resource)]
pub struct OutgoingTx<T> {
    pub(crate) message: Arc<MessageType>,
    pub(crate) _marker: PhantomData<T>,
}