
use bevy::log;
use tokio::sync::mpsc;

pub struct Receiver<T> {
    tx: mpsc::Sender<T>,
    rx: mpsc::Receiver<T>,
}

impl<T> Receiver<T> {
    pub fn new(size: usize) -> Self {
        let (tx, rx) = mpsc::channel(size);
        Self { tx, rx }
    }

    pub fn send(&self, item: T) {
        if let Err(_) = self.tx.try_send(item) {
            log::error!("A Receiver somewhere was full.'")
        }
    }

    pub fn recv(&mut self) -> Option<T> {
        self.rx.try_recv().ok()
    }

    pub fn tx(&self) -> mpsc::Sender<T> {
        self.tx.clone()
    }

    pub fn iter(&mut self) -> impl Iterator<Item=T> {
        self
    }
}

impl<'r, T> Iterator for &'r mut Receiver<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}
