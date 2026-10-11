//! Saved source identity and the currently implemented single-FTMS-trainer contract.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<crate::ble_sensors::Kind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Pairing {
    pub schema: u32,
    pub resistance: Device,
    pub power: Device,
    pub cadence: Option<Device>,
    #[serde(default)]
    pub heart_rate: Option<Device>,
}

impl Pairing {
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() > 32 * 1024 {
            return Err("Pairing exceeds 32 KiB".into());
        }
        let pairing: Self =
            serde_json::from_str(raw).map_err(|e| format!("Invalid pairing: {e}"))?;
        pairing.validate()?;
        Ok(pairing)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.schema, 1 | 2) {
            return Err("Unsupported pairing schema".into());
        }
        for device in [&self.resistance, &self.power]
            .into_iter()
            .chain(self.cadence.as_ref())
            .chain(self.heart_rate.as_ref())
        {
            if device.id.trim().is_empty()
                || device.id.len() > 4096
                || device.id.chars().any(char::is_control)
                || device.name.len() > 512
                || device.name.chars().any(char::is_control)
            {
                return Err("Invalid device identity or name".into());
            }
        }
        use crate::ble_sensors::Kind;
        if self.resistance.protocol.is_some() {
            return Err("Resistance requires FTMS".into());
        }
        let valid = |d: &Device, kinds: &[Kind]| {
            if d.protocol.is_none() {
                d.id == self.resistance.id
            } else {
                self.schema == 2
                    && d.id != self.resistance.id
                    && kinds.contains(&d.protocol.unwrap())
            }
        };
        if !valid(&self.power, &[Kind::CyclingPower])
            || self
                .cadence
                .as_ref()
                .is_some_and(|d| !valid(d, &[Kind::Csc, Kind::CyclingPower]))
        {
            return Err("Invalid power/cadence protocol or source identity".into());
        }
        if self.heart_rate.as_ref().is_some_and(|d| {
            self.schema != 2 || d.protocol != Some(Kind::HeartRate) || d.id == self.resistance.id
        }) {
            return Err("Heart rate requires an independent Bluetooth HR source".into());
        }
        Ok(())
    }

    pub fn check_capabilities(
        &self,
        capabilities: &crate::discovery::Capabilities,
    ) -> Result<(), String> {
        self.validate()?;
        if !capabilities.terrain_control || (self.power.protocol.is_none() && !capabilities.power) {
            return Err("Selected trainer must advertise FTMS terrain control and power; no control requested".into());
        }
        if self.cadence.as_ref().is_some_and(|d| d.protocol.is_none()) && !capabilities.cadence {
            return Err(
                "Selected cadence source no longer advertises cadence; reselect sources".into(),
            );
        }
        Ok(())
    }

    pub fn filter(&self, data: &mut crate::ftms::BikeData) {
        if self.cadence.as_ref().is_none_or(|d| d.protocol.is_some()) {
            data.cadence_rpm = None;
        }
        if self.power.protocol.is_some() {
            data.power_w = None;
        }
        if self.heart_rate.is_some() {
            data.heart_rate_bpm = None;
        }
    }
}

pub fn load(path: &std::path::Path) -> Result<Pairing, String> {
    use std::io::Read;
    let mut raw = String::new();
    std::fs::File::open(path)
        .map_err(|e| format!("Select a trainer in Settings / Devices. Pairing unavailable: {e}"))?
        .take(32 * 1024 + 1)
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())?;
    Pairing::parse(&raw)
}

pub fn save(path: &std::path::Path, pairing: &Pairing) -> Result<(), String> {
    use std::io::Write;
    pairing.validate()?;
    let temporary = path.with_extension("json.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(&serde_json::to_vec_pretty(pairing).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pairing() -> Pairing {
        let device = Device {
            id: "synthetic-two".into(),
            name: "Same name".into(),
            protocol: None,
        };
        Pairing {
            schema: 1,
            resistance: device.clone(),
            power: device.clone(),
            cadence: Some(device),
            heart_rate: None,
        }
    }
    #[test]
    fn identity_not_name_and_capabilities_gate_control() {
        let mut p = pairing();
        p.power.id = "synthetic-one".into();
        assert!(p.validate().is_err());
        p.power.id = p.resistance.id.clone();
        let mut caps = crate::discovery::Capabilities {
            power: true,
            cadence: true,
            terrain_control: true,
            heart_rate: false,
        };
        assert!(p.check_capabilities(&caps).is_ok());
        caps.terrain_control = false;
        assert!(p.check_capabilities(&caps).is_err());
        caps.terrain_control = true;
        caps.cadence = false;
        assert!(p.check_capabilities(&caps).is_err());
        p.cadence = None;
        assert!(p.check_capabilities(&caps).is_ok());
    }
    #[test]
    fn disabled_cadence_does_not_fabricate_power_or_replace_source() {
        let mut p = pairing();
        p.cadence = None;
        let mut data = crate::ftms::BikeData {
            cadence_rpm: Some(83.),
            power_w: Some(0),
            ..Default::default()
        };
        p.filter(&mut data);
        assert_eq!(data.cadence_rpm, None);
        assert_eq!(data.power_w, Some(0));
        data.power_w = None;
        p.filter(&mut data);
        assert_eq!(data.power_w, None);
    }
    #[test]
    fn persistence_replaces_atomically_and_rejects_corrupt_or_unsupported_choices() {
        let path = std::env::temp_dir().join(format!(
            "openride-pairing-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut p = pairing();
        save(&path, &p).unwrap();
        assert_eq!(load(&path).unwrap(), p);
        p.cadence = None;
        save(&path, &p).unwrap();
        assert_eq!(load(&path).unwrap(), p);
        p.schema = 99;
        assert!(save(&path, &p).is_err());
        assert_eq!(load(&path).unwrap().schema, 1);
        std::fs::write(&path, "{broken").unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(load(&path).is_err());
    }
}
