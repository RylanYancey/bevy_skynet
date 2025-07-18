
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
        let _ = self.tx.blocking_send(item);
    }

    pub fn send_expect(&self, item: T, err: &str) {
        if let Err(_) = self.tx.blocking_send(item) {
            panic!("Failed to send an item to a receiver with error: '{err}'")
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
