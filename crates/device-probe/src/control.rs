//! Bounded, one-shot acquisition diagnostic. No start, grade, or ERG writes.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use trainer_core::ftms::{ControlPoint, Features, Outcome, Request};
use windows::{
    core::{Error, Result, HRESULT, HSTRING},
    Devices::Bluetooth::{
        BluetoothConnectionStatus, BluetoothLEDevice,
        GenericAttributeProfile::{
            GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue as Cccd,
            GattCommunicationStatus, GattValueChangedEventArgs, GattWriteOption,
        },
    },
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, DataWriter},
    Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
        System::Threading::CreateMutexW,
    },
};

fn error(message: impl AsRef<str>) -> Error {
    Error::new(HRESULT(0x80004005u32 as i32), message)
}

pub struct Owner(HANDLE);
impl Owner {
    pub fn claim(address: u64) -> Result<Self> {
        let name = HSTRING::from(format!("Local\\OpenRide-Trainer-{address:012x}"));
        let handle = unsafe { CreateMutexW(None, false, &name)? };
        let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let owner = Self(handle);
        if exists {
            return Err(error(
                "Another OpenRide probe owns this trainer in this Windows session",
            ));
        }
        Ok(owner)
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) struct Subscription {
    ch: GattCharacteristic,
    token: i64,
}
impl Subscription {
    pub(crate) fn new(
        ch: &GattCharacteristic,
        mode: Cccd,
        kind: u8,
        tx: mpsc::SyncSender<(u8, Vec<u8>, Instant)>,
        failed: Arc<AtomicBool>,
    ) -> Result<Self> {
        let handler = TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(
            move |_, args| {
                let result = (|| -> Result<()> {
                    let args = args
                        .as_ref()
                        .ok_or_else(|| error("Missing notification arguments"))?;
                    let buffer = args.CharacteristicValue()?;
                    let mut bytes = vec![0; buffer.Length()? as usize];
                    DataReader::FromBuffer(&buffer)?.ReadBytes(&mut bytes)?;
                    if tx.try_send((kind, bytes, Instant::now())).is_err() {
                        failed.store(true, Ordering::SeqCst);
                    }
                    Ok(())
                })();
                if result.is_err() {
                    failed.store(true, Ordering::SeqCst);
                }
                Ok(())
            },
        );
        let guard = Self {
            ch: ch.clone(),
            token: ch.ValueChanged(&handler)?,
        };
        let op = ch.WriteClientCharacteristicConfigurationDescriptorAsync(mode)?;
        let start = Instant::now();
        while op.Status()?.0 == 0 {
            if start.elapsed() > Duration::from_secs(5) {
                let _ = op.Cancel();
                return Err(error("Subscription timed out"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if op.GetResults()? != GattCommunicationStatus::Success {
            return Err(error("Subscription rejected"));
        }
        Ok(guard)
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let _ = self.ch.RemoveValueChanged(self.token);
        // Disconnect follows this diagnostic. Bound best-effort CCCD cleanup.
        if let Ok(op) = self
            .ch
            .WriteClientCharacteristicConfigurationDescriptorAsync(Cccd::None)
        {
            let start = Instant::now();
            while op.Status().is_ok_and(|s| s.0 == 0) && start.elapsed() < Duration::from_secs(1) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = op.Cancel();
        }
    }
}

pub fn acquire(
    device: &BluetoothLEDevice,
    cp: &GattCharacteristic,
    status: &GattCharacteristic,
    features: &Features,
) -> Result<()> {
    let (tx, rx) = mpsc::sync_channel(32);
    let failed = Arc::new(AtomicBool::new(false));
    let _indications = Subscription::new(cp, Cccd::Indicate, 0, tx.clone(), failed.clone())?;
    let _status = Subscription::new(status, Cccd::Notify, 1, tx, failed.clone())?;
    let mut machine = ControlPoint::new(features);
    let clock = Instant::now();
    let (id, bytes) = machine.begin(0, Request::Acquire).map_err(error)?;
    println!(
        "{}",
        serde_json::json!({"event":"control_request", "opcode":0,"bytes":bytes})
    );
    let writer = DataWriter::new()?;
    writer.WriteBytes(&bytes)?;
    let write =
        cp.WriteValueWithOptionAsync(&writer.DetachBuffer()?, GattWriteOption::WriteWithResponse)?;
    let result = (|| -> Result<()> {
        let mut completed = false;
        loop {
            let ms = clock.elapsed().as_millis() as u64;
            machine.tick(ms).map_err(error)?;
            if failed.load(Ordering::SeqCst) {
                return Err(error("Notification error or queue overflow"));
            }
            if device.ConnectionStatus()? != BluetoothConnectionStatus::Connected {
                return Err(error("Trainer disconnected"));
            }
            let mut accepted = false;
            for (kind, bytes, _) in rx.try_iter() {
                println!(
                    "{}",
                    serde_json::json!({"event":if kind == 0 {"control_indication"} else {"machine_status"},"bytes":bytes})
                );
                let outcome = if kind == 0 {
                    machine.indication(ms, &bytes).map_err(error)?
                } else {
                    machine.machine_status(&bytes).map_err(error)?;
                    machine.tick(ms).map_err(error)?;
                    None
                };
                match outcome {
                    Some(Outcome::Accepted(_)) => accepted = true,
                    Some(Outcome::Rejected { code, .. }) => {
                        return Err(error(format!(
                            "Trainer rejected control with FTMS result {code}"
                        )))
                    }
                    None => (),
                }
            }
            if !completed && write.Status()?.0 != 0 {
                let ok = write.GetResults()? == GattCommunicationStatus::Success;
                println!(
                    "{}",
                    serde_json::json!({"event":"control_write_complete","success":ok})
                );
                completed = true;
                match machine.write_complete(ms, id, ok).map_err(error)? {
                    Some(Outcome::Accepted(_)) => accepted = true,
                    Some(Outcome::Rejected { code, .. }) => {
                        return Err(error(format!(
                            "Trainer rejected control with FTMS result {code}"
                        )))
                    }
                    None => (),
                }
            }
            if accepted {
                println!(
                    "{}",
                    serde_json::json!({"event":"control_acquired","start_sent":false,"resistance_changed":false})
                );
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if result.is_err() {
        machine.invalidate();
        let _ = write.Cancel();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_is_exclusive_and_released_on_drop() {
        let address = 0xff0000000000 | u64::from(std::process::id());
        let owner = Owner::claim(address).unwrap();
        assert!(Owner::claim(address).is_err());
        drop(owner);
        assert!(Owner::claim(address).is_ok());
    }
}
