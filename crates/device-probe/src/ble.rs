use serde_json::json;
use windows::{
    core::Result,
    Devices::{
        Bluetooth::GenericAttributeProfile::{
            GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
            GattCommunicationStatus, GattValueChangedEventArgs,
        },
        Bluetooth::{BluetoothCacheMode, BluetoothLEDevice},
        Enumeration::DeviceInformation,
    },
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, IBuffer},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

fn bytes(buffer: &IBuffer) -> Result<Vec<u8>> {
    let reader = DataReader::FromBuffer(buffer)?;
    let mut data = vec![0; buffer.Length()? as usize];
    reader.ReadBytes(&mut data)?;
    Ok(data)
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

pub fn run() -> Result<()> {
    println!("{}", json!({"event":"stage", "name":"initialize_winrt"}));
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let _apartment = Apartment;
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--address") {
        let address = args
            .get(2)
            .and_then(|a| u64::from_str_radix(&a.replace(':', ""), 16).ok())
            .filter(|a| *a <= 0xffffffffffff)
            .ok_or_else(|| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80070057u32 as i32),
                    "Expected a 48-bit hexadecimal Bluetooth address",
                )
            })?;
        println!("{}", json!({"event":"stage", "name":"open_known_address"}));
        return inspect(BluetoothLEDevice::FromBluetoothAddressAsync(address)?.join()?);
    }
    let target = args.get(1);
    println!(
        "{}",
        json!({"event":"stage", "name":"paired_device_selector"})
    );
    let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
    println!(
        "{}",
        json!({"event":"stage", "name":"enumerate_paired_devices"})
    );
    let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.join()?;
    let mut matches = Vec::new();
    for info in devices {
        let name = info.Name()?.to_string();
        println!("{}", json!({"event":"paired_device", "name":name}));
        if target.is_some_and(|t| t.eq_ignore_ascii_case(&name)) {
            matches.push(info);
        }
    }
    if target.is_none() {
        return Ok(());
    }
    if matches.len() != 1 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80070057u32 as i32),
            "Expected exactly one paired device with that name",
        ));
    }
    inspect(BluetoothLEDevice::FromIdAsync(&matches[0].Id()?)?.join()?)
}

fn inspect(device: BluetoothLEDevice) -> Result<()> {
    let acquire = std::env::args().any(|a| {
        a == "--acquire-control"
            || a.starts_with("--session-check=")
            || a.starts_with("--resistance-check=")
            || a.starts_with("--pedal-check=")
            || a.starts_with("--live-session=")
    });
    let _owner = if acquire {
        Some(crate::control::Owner::claim(device.BluetoothAddress()?)?)
    } else {
        None
    };
    let result = inspect_connected(&device, acquire);
    let closed = device.Close();
    println!(
        "{}",
        json!({"event":"device_closed", "success":closed.is_ok()})
    );
    result.and(closed)
}

fn inspect_connected(device: &BluetoothLEDevice, acquire: bool) -> Result<()> {
    println!(
        "{}",
        json!({"event":"device", "name":device.Name()?.to_string(), "connection":format!("{:?}",device.ConnectionStatus()?)})
    );
    let result = device
        .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
        .join()?;
    if result.Status()? != GattCommunicationStatus::Success {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            format!("Service discovery: {:?}", result.Status()?),
        ));
    }
    let services = result.Services()?;
    let mut bike: Option<GattCharacteristic> = None;
    let mut control = None;
    let mut machine_status = None;
    let mut features = None;
    for service in &services {
        let service_uuid = service.Uuid()?;
        println!(
            "{}",
            json!({"event":"service", "uuid":format!("{service_uuid:?}")})
        );
        let chars = service
            .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .join()?;
        if chars.Status()? != GattCommunicationStatus::Success {
            println!(
                "{}",
                json!({"event":"characteristic_error", "status":format!("{:?}",chars.Status()?)})
            );
            continue;
        }
        for ch in chars.Characteristics()? {
            let uuid = ch.Uuid()?;
            println!(
                "{}",
                json!({"event":"characteristic", "uuid":format!("{uuid:?}"), "properties": ch.CharacteristicProperties()?.0})
            );
            // Only read standard capabilities / device-information fields.
            let short = uuid.data1;
            let is_standard = uuid.data2 == 0
                && uuid.data3 == 0x1000
                && uuid.data4 == [0x80, 0, 0, 0x80, 0x5f, 0x9b, 0x34, 0xfb];
            if is_standard
                && matches!(
                    short,
                    0x2acc | 0x2ad4..=0x2ad8 | 0x2a26 | 0x2a27 | 0x2a28 | 0x2a29 | 0x2a24
                )
            {
                let read = ch
                    .ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?
                    .join()?;
                if read.Status()? == GattCommunicationStatus::Success {
                    let data = bytes(&read.Value()?)?;
                    if short == 0x2acc {
                        features = trainer_core::ftms::features(&data).ok();
                        println!(
                            "{}",
                            json!({"event":"decoded_features", "result":trainer_core::ftms::features(&data)})
                        );
                    }
                    println!(
                        "{}",
                        json!({"event":"read", "uuid":format!("{uuid:?}"), "bytes":data,
                        "text":if matches!(short, 0x2a24 | 0x2a26..=0x2a29) {Some(String::from_utf8_lossy(&data).to_string())} else {None}})
                    );
                } else {
                    println!(
                        "{}",
                        json!({"event":"read_error", "uuid":format!("{uuid:?}"), "status":format!("{:?}",read.Status()?)})
                    );
                }
            }
            if is_standard && short == 0x2ad2 {
                bike = Some(ch.clone());
            }
            if is_standard && short == 0x2ad9 {
                control = Some(ch.clone());
            }
            if is_standard && short == 0x2ada {
                machine_status = Some(ch);
            }
        }
    }
    if let Some(ch) = bike.clone() {
        let (tx, rx) = std::sync::mpsc::sync_channel(128);
        let handler = TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(
            move |_, args| {
                if let Some(args) = args.as_ref() {
                    if let Ok(data) = args.CharacteristicValue().and_then(|v| bytes(&v)) {
                        let _ = tx.try_send(data);
                    }
                }
                Ok(())
            },
        );
        let token = ch.ValueChanged(&handler)?;
        let status = ch
            .WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::Notify,
            )?
            .join()?;
        println!(
            "{}",
            json!({"event":"subscription", "status":format!("{status:?}")})
        );
        if status == GattCommunicationStatus::Success {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut count = 0;
            while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
                match rx.recv_timeout(remaining) {
                    Ok(data) => {
                        count += 1;
                        println!(
                            "{}",
                            json!({"event":"indoor_bike_data", "bytes":data, "decoded":trainer_core::ftms::indoor_bike(&data)})
                        );
                    }
                    Err(_) => break,
                }
            }
            println!(
                "{}",
                json!({"event":if acquire {"telemetry_capture_complete"} else {"capture_complete"}, "packets":count, "control_requested":false})
            );
            let status = ch
                .WriteClientCharacteristicConfigurationDescriptorAsync(
                    GattClientCharacteristicConfigurationDescriptorValue::None,
                )?
                .join()?;
            println!(
                "{}",
                json!({"event":"unsubscribe", "status":format!("{status:?}")})
            );
        }
        ch.RemoveValueChanged(token)?;
    } else {
        println!("{}", json!({"event":"no_indoor_bike_data"}));
    }
    let control_result = if acquire {
        match (control, machine_status, features) {
            (Some(cp), Some(status), Some(features)) => {
                if let Some(path) = std::env::args().find_map(|a| {
                    a.strip_prefix("--session-check=")
                        .or_else(|| a.strip_prefix("--resistance-check="))
                        .or_else(|| a.strip_prefix("--pedal-check="))
                        .or_else(|| a.strip_prefix("--live-session="))
                        .map(str::to_owned)
                }) {
                    let bike = bike.as_ref().ok_or_else(|| {
                        windows::core::Error::new(
                            windows::core::HRESULT(0x80004005u32 as i32),
                            "Session check requires Indoor Bike Data",
                        )
                    })?;
                    crate::continuous::run(
                        device,
                        &cp,
                        &status,
                        bike,
                        &features,
                        std::path::Path::new(&path),
                        if std::env::args().any(|a| a.starts_with("--live-session=")) {
                            trainer_core::diagnostic::CheckMode::Live
                        } else if std::env::args().any(|a| a.starts_with("--pedal-check=")) {
                            trainer_core::diagnostic::CheckMode::Pedaling
                        } else if std::env::args().any(|a| a.starts_with("--resistance-check=")) {
                            trainer_core::diagnostic::CheckMode::Stationary
                        } else {
                            trainer_core::diagnostic::CheckMode::StartPause
                        },
                    )
                } else {
                    crate::control::acquire(device, &cp, &status, &features)
                }
            }
            _ => Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                "Required FTMS control/status/features unavailable",
            )),
        }
    } else {
        Ok(())
    };
    for service in services {
        service.Close()?;
    }
    control_result
}
