
use std::{io, marker::PhantomData};

use bevy::{ecs::system::SystemParam, prelude::*};
use serde::{de::DeserializeOwned, Serialize};

use crate::{backends::{Backend, IBackend, UserId}, comms::{IncomingRx, OutgoingTx}, context::Message};

/// Receiver for network messages of a given type.
/// Reading Network messages consumes them. Future reads
/// of the receiver will not yield the message, even within 
/// the same frame. 
#[derive(SystemParam)]
pub struct NetReceiver<'w, 's, T> 
where
    T: DeserializeOwned + TypePath + Send + Sync + 'static
{
    rx: ResMut<'w, IncomingRx<T>>,
    _marker: PhantomData<&'s ()>,
}

impl<'w, 's, T> NetReceiver<'w, 's, T>
where
    T: DeserializeOwned + TypePath + Send + Sync + 'static
{
    pub fn recv(&mut self) -> Option<Message<T>> {
        self.rx.rx.try_recv().ok()
    }
}

impl<'w, 's, T> Iterator for NetReceiver<'w, 's, T>
where
    T: DeserializeOwned + TypePath + Send + Sync + 'static
{
    type Item = Message<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}

/// Transmitter for network messages of a type.
/// Supports broadcasting and sending to individuals. 
#[derive(SystemParam)]
pub struct NetSender<'w, 's, T> 
where
    T: Serialize + TypePath + Send + Sync + 'static
{
    backend: Res<'w, Backend>,
    tx: Res<'w, OutgoingTx<T>>,
    buf: Local<'s, Vec<u8>>,
}

impl<'w, 's, T> NetSender<'w, 's, T>
where
    T: Serialize + TypePath + Send + Sync + 'static
{
    fn write_buffer(&mut self, message: &T) {
        self.buf.clear();
        let msg_id = self.tx.message.id.to_be_bytes();
        self.buf.extend_from_slice(&msg_id);
        ciborium::into_writer(message, &mut*self.buf).unwrap();
    }

    /// Broadcast a message to all connected users. 
    pub fn broadcast(&mut self, message: &T) {  
        self.write_buffer(message);
        self.backend.broadcast_packet(&self.buf);
    }

    /// Send a message to the user.
    pub fn send(&mut self, to: UserId, message: &T) {
        self.write_buffer(message);
        self.backend.send_packet(to, &self.buf);
    }
}

