//! Signal wrapper

#[cfg(feature = "tokio")]
pub use tokio::sync::mpsc::{UnboundedReceiver as Receiver, UnboundedSender as Sender, unbounded_channel as channel};
#[cfg(all(feature = "crossbeam-channel", not(feature = "tokio")))]
pub use crossbeam_channel::{Receiver, Sender, unbounded as channel};
#[cfg(not(any(feature = "crossbeam-channel", feature = "tokio")))]
pub use std::sync::mpsc::{Receiver, Sender, channel};

#[repr(transparent)]
pub struct SignalSender(pub Sender<()>);

#[repr(transparent)]
pub struct SignalReceiver(pub Receiver<()>);

impl SignalSender {
    pub fn send(&self) { let _ = self.0.send(()); }
}

impl SignalReceiver {
    pub fn recv(&mut self) {
        #[cfg(feature = "tokio")]
        let _ = self.0.blocking_recv();
        
        #[cfg(not(feature = "tokio"))]
        let _ = self.0.recv();
    }
}

pub fn new_signal() -> (SignalSender, SignalReceiver) {
    let (s, r) = channel();
    
    (SignalSender(s), SignalReceiver(r))
}
