use serde_json::json;
use windows::{
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
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::Result,
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
    let discovery = args
        .get(1)
        .is_some_and(|a| a == "--discover" || a == "--inspect-id");
    if discovery {
        println!(
            "{}",
            json!({"event":"diagnostic_manifest", "schema":1,
            "app_version":env!("CARGO_PKG_VERSION"), "platform":std::env::consts::OS,
            "architecture":std::env::consts::ARCH, "mode":args[1],
            "control_requested":false, "telemetry_verified":false,
            "privacy":"Contains device names, Windows device IDs and capability/device-information fields. No routes, activity history, network keys or credentials."})
        );
    }
    if args.get(1).is_some_and(|a| a == "--inspect-id") {
        return inspect(
            BluetoothLEDevice::FromIdAsync(&windows::core::HSTRING::from(&args[2]))?.join()?,
        );
    }
    if args.get(1).is_some_and(|a| a == "--device-id") {
        let selected = crate::selected_pairing(&args)
            .map_err(|message| {
                windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), message)
            })?
            .ok_or_else(|| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80070057u32 as i32),
                    "Explicit device selection requires pairing",
                )
            })?;
        return inspect(
            BluetoothLEDevice::FromIdAsync(&windows::core::HSTRING::from(&selected.resistance.id))?
                .join()?,
        );
    }
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
    let target = if discovery { None } else { args.get(1) };
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
        println!(
            "{}",
            json!({"event":"paired_device", "name":name, "id":info.Id()?.to_string(), "transport":"bluetooth_le"})
        );
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
        json!({"event":"device", "name":device.Name()?.to_string(), "id":device.DeviceId()?.to_string(), "connection":format!("{:?}",device.ConnectionStatus()?)})
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
    let mut csc_features = None;
    let mut power_features = None;
    let mut standard_characteristics = Vec::new();
    let mut complete = true;
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
            complete = false;
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
            if is_standard {
                standard_characteristics.push((short, ch.CharacteristicProperties()?.0 as u32));
            }
            if is_standard
                && matches!(
                    short,
                    0x2acc | 0x2ad4..=0x2ad8 | 0x2a26 | 0x2a27 | 0x2a28 | 0x2a29 | 0x2a24 | 0x2a5c | 0x2a65
                )
            {
                let read = ch
                    .ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?
                    .join()?;
                if read.Status()? == GattCommunicationStatus::Success {
                    let data = bytes(&read.Value()?)?;
                    if short == 0x2a5c && data.len() == 2 {
                        csc_features = Some(u16::from_le_bytes([data[0], data[1]]));
                    }
                    if short == 0x2a65 && data.len()==4 { power_features=Some(u32::from_le_bytes(data[..].try_into().unwrap())); }
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
    if std::env::args().any(|a| a == "--inspect-id") {
        let has=|number| standard_characteristics.iter().any(|(id,properties)| *id==number && properties&0x10!=0);
        let sensor_roles=json!({
            "power":if has(0x2a63) { Some("cycling_power") } else { None },
            "cadence":if has(0x2a5b) && csc_features.is_some_and(|f| f&2!=0) { Some("csc") }
                else if has(0x2a63) && power_features.is_some_and(|f| f&8!=0) { Some("cycling_power") } else { None },
            "heart_rate":if has(0x2a37) { Some("heart_rate") } else { None }
        });
        let ftms_characteristics: Vec<_> = standard_characteristics
            .iter()
            .copied()
            .filter(|(id, _)| matches!(id, 0x2ad2 | 0x2ad9 | 0x2ada))
            .collect();
        println!(
            "{}",
            json!({"event":"capabilities", "id":device.DeviceId()?.to_string(),
            "roles":trainer_core::discovery::capabilities(&standard_characteristics, features.as_ref(), csc_features),
            "live_roles":trainer_core::discovery::capabilities(&ftms_characteristics, features.as_ref(), None),
            "sensor_roles":sensor_roles,
            "characteristic_discovery_complete":complete, "telemetry_verified":false, "control_verified":false,
            "note":"Roles describe discovered standard capabilities, not verified operation. False means capability not established; unknown/vendor protocols may require an adapter."})
        );
        for service in services {
            service.Close()?;
        }
        return Ok(());
    }
    let selected =
        crate::selected_pairing(&std::env::args().collect::<Vec<_>>()).map_err(|message| {
            windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), message)
        })?;
    if let Some(selected) = &selected {
        let ftms_characteristics: Vec<_> = standard_characteristics
            .iter()
            .copied()
            .filter(|(id, _)| matches!(id, 0x2ad2 | 0x2ad9 | 0x2ada))
            .collect();
        let caps =
            trainer_core::discovery::capabilities(&ftms_characteristics, features.as_ref(), None);
        let validation = if device.DeviceId()?.to_string() != selected.resistance.id {
            Err("Connected device differs from selected identity".to_owned())
        } else {
            selected.check_capabilities(&caps)
        };
        if let Err(message) = validation {
            for service in services {
                service.Close()?;
            }
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80070057u32 as i32),
                message,
            ));
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
                            json!({"event":"indoor_bike_data", "bytes":data, "decoded":trainer_core::ftms::indoor_bike(&data).map(|mut decoded| {
                                if let Some(pairing) = &selected { pairing.filter(&mut decoded); }
                                decoded
                            })})
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
