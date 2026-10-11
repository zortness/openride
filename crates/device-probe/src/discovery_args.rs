/// Diagnostic modes accept no additional options, especially control flags.
pub fn validate(args: &[String]) -> Result<(), &'static str> {
    if !args
        .iter()
        .any(|a| a == "--discover" || a == "--inspect-id")
    {
        return Ok(());
    }
    match args.get(1).map(String::as_str) {
        Some("--discover") if args.len() == 2 => Ok(()),
        Some("--inspect-id")
            if args.len() == 3 && !args[2].is_empty() && !args[2].starts_with("--") =>
        {
            Ok(())
        }
        _ => Err(
            "Use --discover alone or --inspect-id WINDOWS_DEVICE_ID; diagnostic modes cannot be combined with other options",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_reject_every_control_mode_and_alternate_dispatch() {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(validate(&args(&["probe", "--discover"])).is_ok());
        assert!(validate(&args(&["probe", "--inspect-id", "synthetic-id"])).is_ok());
        for flag in [
            "--acquire-control",
            "--live-session=x",
            "--pedal-check=x",
            "--ant-journal=x",
            "--replay",
            "--ui-control",
        ] {
            assert!(validate(&args(&["probe", "--inspect-id", "synthetic-id", flag])).is_err());
            assert!(validate(&args(&["probe", flag, "--discover"])).is_err());
        }
        assert!(validate(&args(&["probe", "--inspect-id", ""])).is_err());
        assert!(validate(&args(&["probe", "--inspect-id", "--acquire-control"])).is_err());
    }
}
