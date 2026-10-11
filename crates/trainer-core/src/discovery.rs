//! Capability evidence for pairing. Discovery is not proof of working telemetry/control.
use serde::Serialize;

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Capabilities {
    pub power: bool,
    pub cadence: bool,
    pub heart_rate: bool,
    pub terrain_control: bool,
}

/// Only standard characteristics with usable properties count. Vendor UUIDs must
/// be filtered by the transport before passing their assigned numbers here.
pub fn capabilities(
    characteristics: &[(u32, u32)],
    ftms: Option<&crate::ftms::Features>,
    csc_features: Option<u16>,
) -> Capabilities {
    let notify = |uuid| {
        characteristics
            .iter()
            .any(|&(id, props)| id == uuid && props & 0x10 != 0)
    };
    let control = characteristics
        .iter()
        .any(|&(id, props)| id == 0x2ad9 && props & 0x28 == 0x28);
    let bike = notify(0x2ad2);
    Capabilities {
        power: notify(0x2a63) || (bike && ftms.is_some_and(|f| f.power)),
        cadence: (notify(0x2a5b) && csc_features.is_some_and(|f| f & 2 != 0))
            || (bike && ftms.is_some_and(|f| f.cadence)),
        heart_rate: notify(0x2a37),
        terrain_control: control && notify(0x2ada) && bike && ftms.is_some_and(|f| f.simulation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_features_and_wrong_properties_do_not_advertise_roles() {
        assert_eq!(
            capabilities(&[(0x2ad2, 0x10), (0x2ad9, 0x28)], None, None),
            Capabilities::default()
        );
        assert_eq!(
            capabilities(&[(0x2a63, 2), (0x2a37, 2), (0x2a5b, 0x10)], None, Some(1)),
            Capabilities::default()
        );
    }

    #[test]
    fn independent_sensors_and_ftms_require_their_evidence() {
        let sensors = capabilities(
            &[(0x2a63, 0x10), (0x2a37, 0x10), (0x2a5b, 0x10)],
            None,
            Some(2),
        );
        assert_eq!(
            sensors,
            Capabilities {
                power: true,
                cadence: true,
                heart_rate: true,
                terrain_control: false
            }
        );
        let features = crate::ftms::features(&[2, 64, 0, 0, 0, 32, 0, 0]).unwrap();
        let mut chars = vec![(0x2ad2, 0x10), (0x2ad9, 0x28)];
        assert!(!capabilities(&chars, Some(&features), None).terrain_control);
        chars.push((0x2ada, 0x10));
        assert_eq!(
            capabilities(&chars, Some(&features), None),
            Capabilities {
                power: true,
                cadence: true,
                heart_rate: false,
                terrain_control: true
            }
        );
    }
}
