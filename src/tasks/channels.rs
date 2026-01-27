use tokio::sync::mpsc;

use crate::app::StationInfo;
use crate::audio::LoopBuffer;

/// Commands from TUI/input to producer task
#[derive(Debug)]
pub enum ProducerCommand {
    /// Skip to next station immediately
    NextStation,
    /// Audio device changed - main loop should recreate playback engine
    AudioDeviceChanged(usize),
    /// Shutdown the producer
    Quit,
}

/// Messages from producer to main loop
#[derive(Debug)]
pub enum ProducerEvent {
    /// New station selected
    StationSelected(StationInfo),
    /// New loop ready for playback
    LoopReady(LoopBuffer, StationInfo),
    /// Error occurred (station skipped)
    #[allow(dead_code)]
    Error(String),
    /// Audio device changed - main loop should recreate playback engine
    AudioDeviceChanged(usize),
    /// Producer is shutting down
    Shutdown,
}

/// Channel bundle for communication
pub struct Channels {
    /// Commands to producer
    pub cmd_tx: mpsc::Sender<ProducerCommand>,
    pub cmd_rx: mpsc::Receiver<ProducerCommand>,

    /// Events from producer
    pub event_tx: mpsc::Sender<ProducerEvent>,
    pub event_rx: mpsc::Receiver<ProducerEvent>,
}

impl Channels {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(8);
        let (event_tx, event_rx) = mpsc::channel(8);

        Self {
            cmd_tx,
            cmd_rx,
            event_tx,
            event_rx,
        }
    }

    /// Split into sender/receiver pairs
    pub fn split(
        self,
    ) -> (
        mpsc::Sender<ProducerCommand>,
        mpsc::Receiver<ProducerCommand>,
        mpsc::Sender<ProducerEvent>,
        mpsc::Receiver<ProducerEvent>,
    ) {
        (self.cmd_tx, self.cmd_rx, self.event_tx, self.event_rx)
    }
}

impl Default for Channels {
    fn default() -> Self {
        Self::new()
    }
}
