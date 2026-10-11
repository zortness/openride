//! Receive-only, independently timed BLE peripherals. Never owns a control point.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
use trainer_core::ble_sensors::{Decoder, Kind, Values};
use windows::{
    Devices::Bluetooth::{
        BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
        GenericAttributeProfile::{
            GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue as Cccd,
            GattCommunicationStatus, GattDeviceService,
        },
    },
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::{Error, HRESULT, HSTRING, Result},
};

pub struct Event {
    pub id: String,
    pub kind: Kind,
    pub received_ms: u64,
    pub values: Values,
    pub bytes: Vec<u8>,
    pub error: Option<String>,
}
pub struct Worker {
    pub rx: mpsc::Receiver<Event>,
    stop: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Worker {
    pub fn start(id: String, kinds: Vec<Kind>, clock: Instant) -> Self {
        let (tx, rx) = mpsc::sync_channel(128);
        let (done_tx, done) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        std::thread::spawn(move || {
            let result = receive(&id, &kinds, clock, &cancelled, &tx);
            if let Err(error) = result {
                for kind in kinds {
                    let _ = tx.try_send(Event {
                        id: id.clone(),
                        kind,
                        received_ms: clock.elapsed().as_millis() as u64,
                        values: Values::default(),
                        bytes: vec![],
                        error: Some(error.to_string()),
                    });
                }
            }
            let _ = done_tx.send(());
        });
        Self { rx, stop, done }
    }
    pub fn stop(&self) -> bool {
        self.stop.store(true, Ordering::Relaxed);
        self.done.recv_timeout(Duration::from_secs(5)).is_ok()
    }
}
fn error(message: impl ToString) -> Error {
    Error::new(HRESULT(0x80004005u32 as i32), message.to_string())
}
macro_rules! wait {
    ($operation:expr,$stop:expr) => {{
        let op = $operation;
        let since = Instant::now();
        while op.Status()?.0 == 0 {
            if $stop.load(Ordering::Relaxed) || since.elapsed() > Duration::from_secs(8) {
                let _ = op.Cancel();
                return Err(error("Sensor operation cancelled or timed out"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        op.GetResults()
    }};
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}
struct Connection {
    device: BluetoothLEDevice,
    services: Vec<GattDeviceService>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        for service in &self.services {
            let _ = service.Close();
        }
        let _ = self.device.Close();
    }
}
fn standard(uuid: windows::core::GUID, number: u32) -> bool {
    uuid.data1 == number
        && uuid.data2 == 0
        && uuid.data3 == 0x1000
        && uuid.data4 == [0x80, 0, 0, 0x80, 0x5f, 0x9b, 0x34, 0xfb]
}
fn receive(
    id: &str,
    kinds: &[Kind],
    clock: Instant,
    stop: &AtomicBool,
    tx: &mpsc::SyncSender<Event>,
) -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let _apartment = Apartment;
    let device = wait!(BluetoothLEDevice::FromIdAsync(&HSTRING::from(id))?, stop)?;
    let mut connection = Connection {
        device,
        services: vec![],
    };
    if connection.device.DeviceId()?.to_string() != id {
        return Err(error("Sensor identity mismatch"));
    }
    let services = wait!(
        connection
            .device
            .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?,
        stop
    )?;
    if services.Status()? != GattCommunicationStatus::Success {
        return Err(error("Sensor service discovery failed"));
    }
    for service in services.Services()? {
        connection.services.push(service);
    }
    let (notification_tx, notification_rx) = mpsc::sync_channel(128);
    let failed = Arc::new(AtomicBool::new(false));
    let mut subscriptions = vec![];
    for (index, kind) in kinds.iter().enumerate() {
        let mut candidates: Vec<GattCharacteristic> = vec![];
        for service in &connection.services {
            if !standard(service.Uuid()?, kind.service()) {
                continue;
            }
            let chars = wait!(
                service.GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?,
                stop
            )?;
            if chars.Status()? != GattCommunicationStatus::Success {
                return Err(error("Sensor characteristic discovery failed"));
            }
            for ch in chars.Characteristics()? {
                if standard(ch.Uuid()?, kind.characteristic())
                    && ch.CharacteristicProperties()?.0 & 0x10 != 0
                {
                    candidates.push(ch);
                }
            }
        }
        if candidates.len() != 1 {
            return Err(error(format!(
                "Expected one notifying {:?} measurement characteristic",
                kind
            )));
        }
        subscriptions.push(crate::control::Subscription::new(
            &candidates[0],
            Cccd::Notify,
            index as u8,
            notification_tx.clone(),
            failed.clone(),
        )?);
    }
    let mut decoders: Vec<_> = kinds.iter().map(|_| Decoder::default()).collect();
    while !stop.load(Ordering::Relaxed) {
        if failed.load(Ordering::SeqCst) {
            return Err(error("Sensor notification queue failed"));
        }
        if connection.device.ConnectionStatus()? != BluetoothConnectionStatus::Connected {
            return Err(error("Selected sensor disconnected; reconnect explicitly"));
        }
        match notification_rx.recv_timeout(Duration::from_millis(100)) {
            Ok((index, bytes, received)) => {
                let index = index as usize;
                let ms = received.saturating_duration_since(clock).as_millis() as u64;
                match decoders[index].decode(kinds[index], &bytes, ms) {
                    Ok(values) => tx
                        .try_send(Event {
                            id: id.into(),
                            kind: kinds[index],
                            received_ms: ms,
                            values,
                            bytes,
                            error: None,
                        })
                        .map_err(|_| error("Sensor event queue full"))?,
                    Err(message) => {
                        tx.try_send(Event {
                            id: id.into(),
                            kind: kinds[index],
                            received_ms: ms,
                            values: Values::default(),
                            bytes,
                            error: Some(message),
                        })
                        .map_err(|_| error("Sensor event queue full"))?;
                        // Decoder errors invalidate this reading; subsequent valid notifications may recover.
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(_) => return Err(error("Sensor notification channel closed")),
        }
    }
    drop(subscriptions); // bounded CCCD cleanup before closing services/device
    Ok(())
}
