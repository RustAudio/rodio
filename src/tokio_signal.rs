
#[cfg(not(feature = "tokio"))]
mod no_tokio {
    pub struct TokioSender;
    
    impl TokioSender {
        pub fn new() -> Self { Self }
        pub fn send(&self) {}
    }
}

#[cfg(not(feature = "tokio"))]
pub use no_tokio::TokioSender;

#[cfg(feature = "tokio")]
mod tokio {
    use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

    pub struct TokioSender(pub UnboundedSender<()>);
    
    impl TokioSender {
        pub fn new() -> Self { Self(unbounded_channel().0) }
        pub fn send(&self) { let _ = self.0.send(()); }
    }
}

#[cfg(feature = "tokio")]
pub use tokio::TokioSender;
