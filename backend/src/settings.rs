use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SETTINGS_VERSION: u32 = 1;
const MAX_INCOMING_PIN_LENGTH: usize = 64;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    version: u32,
    #[serde(default)]
    pub incoming_pin: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            incoming_pin: None,
        }
    }
}

pub fn valid_incoming_pin(pin: &str) -> bool {
    !pin.is_empty()
        && pin.len() <= MAX_INCOMING_PIN_LENGTH
        && pin
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-'))
}

fn validate(settings: &Settings) -> Result<()> {
    if settings.version != SETTINGS_VERSION {
        return Err(anyhow!("unsupported security settings version"));
    }
    if settings
        .incoming_pin
        .as_deref()
        .is_some_and(|pin| !valid_incoming_pin(pin))
    {
        return Err(anyhow!("invalid incoming PIN configuration"));
    }
    Ok(())
}

pub fn state_dir(home: &Path) -> PathBuf {
    std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".local/state"))
        .join("omarchy-nearby")
}

pub fn ensure_private_state_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).context("could not create Nearby state directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .context("could not protect Nearby state directory")?;
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<Settings> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => return Err(error).context("could not read security settings"),
    };
    let settings: Settings =
        serde_json::from_slice(&data).context("could not parse security settings")?;
    validate(&settings)?;
    Ok(settings)
}

pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    validate(settings)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("security settings path has no parent"))?;
    ensure_private_state_dir(parent)?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".settings.json.tmp-{}-{sequence}",
        std::process::id()
    ));
    let data = serde_json::to_vec_pretty(settings).context("could not serialize settings")?;

    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .context("could not create temporary security settings")?;
        file.write_all(&data)
            .context("could not write security settings")?;
        file.write_all(b"\n")
            .context("could not finish security settings")?;
        file.sync_all()
            .context("could not flush security settings")?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .context("could not protect security settings")?;
        }
        fs::rename(&temporary, path).context("could not replace security settings")?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn updated(current: &Settings, incoming_pin: Option<String>) -> Result<Settings> {
    let next = Settings {
        version: current.version,
        incoming_pin,
    };
    validate(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omarchy-nearby-settings-{name}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn incoming_pin_profile_is_exact() {
        for pin in ["1", "123456", "Abc-_.~09", &"x".repeat(64)] {
            assert!(valid_incoming_pin(pin), "expected valid PIN: {pin}");
        }
        for pin in [
            "",
            "with space",
            "a+b",
            "a&b",
            "contraseña",
            &"x".repeat(65),
        ] {
            assert!(!valid_incoming_pin(pin), "expected invalid PIN: {pin}");
        }
    }

    #[test]
    fn missing_settings_are_disabled_and_valid_settings_round_trip() {
        let directory = test_directory("round-trip");
        let path = directory.join("settings.json");
        assert_eq!(load(&path).unwrap(), Settings::default());

        let settings = updated(&Settings::default(), Some("Abc-123".to_string())).unwrap();
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_unsupported_and_invalid_settings_fail_closed() {
        let directory = test_directory("invalid");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");

        for data in [
            b"not json".as_slice(),
            br#"{"version":2,"incomingPin":null}"#,
            br#"{"version":1,"incomingPin":"a+b"}"#,
        ] {
            fs::write(&path, data).unwrap();
            assert!(load(&path).is_err());
        }

        fs::write(
            &path,
            br#"{"version":1,"incomingPin":"Safe-1","futureField":true}"#,
        )
        .unwrap();
        assert_eq!(load(&path).unwrap().incoming_pin.as_deref(), Some("Safe-1"));
        fs::remove_dir_all(directory).unwrap();
    }
}
