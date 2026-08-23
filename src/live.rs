use crate::benchmark::BenchmarkAnchor;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct LiveFragment {
    pub data: Arc<[u8]>,
    pub anchor: Option<BenchmarkAnchor>,
}

struct DeviceLive {
    init: Arc<[u8]>,
    sender: broadcast::Sender<LiveFragment>,
}

#[derive(Default)]
pub struct LiveStreams {
    devices: Mutex<BTreeMap<String, DeviceLive>>,
}

pub struct LiveSubscription {
    pub init: Arc<[u8]>,
    pub receiver: broadcast::Receiver<LiveFragment>,
}

impl LiveStreams {
    pub fn set_init(&self, device_id: &str, data: &[u8]) {
        let mut devices = self
            .devices
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let sender = devices
            .get(device_id)
            .map(|stream| stream.sender.clone())
            .unwrap_or_else(|| broadcast::channel(16).0);
        devices.insert(
            device_id.to_owned(),
            DeviceLive {
                init: Arc::from(data),
                sender,
            },
        );
    }

    pub fn broadcast(&self, device_id: &str, data: &[u8], anchor: Option<BenchmarkAnchor>) {
        let devices = self
            .devices
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(stream) = devices.get(device_id) {
            let _ = stream.sender.send(LiveFragment {
                data: Arc::from(data),
                anchor,
            });
        }
    }

    pub fn subscribe(&self, device_id: &str) -> Option<LiveSubscription> {
        self.devices
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(device_id)
            .map(|stream| LiveSubscription {
                init: stream.init.clone(),
                receiver: stream.sender.subscribe(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscriber_gets_current_init_and_new_fragment() {
        let streams = LiveStreams::default();
        streams.set_init("front", b"init");
        let mut subscription = streams.subscribe("front").unwrap();
        streams.broadcast("front", b"fragment", None);
        assert_eq!(subscription.init.as_ref(), b"init");
        assert_eq!(
            subscription.receiver.recv().await.unwrap().data.as_ref(),
            b"fragment"
        );
    }
}
