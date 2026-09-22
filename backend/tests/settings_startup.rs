#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn unsafe_settings_fail_before_receiver_ready_without_rewriting_state() {
        let base = std::env::temp_dir().join(format!(
            "nearby-startup-settings-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let state = base.join("state/omarchy-nearby");
        fs::create_dir_all(&state).unwrap();
        let target = base.join("valid.json");
        let original = br#"{"version":1,"incomingPin":"Safe-1"}"#;
        fs::write(&target, original).unwrap();
        let settings = state.join("settings.json");
        symlink(&target, &settings).unwrap();

        let mut child = Command::new(env!("CARGO_BIN_EXE_omarchy-nearby-helper"))
            .env("HOME", &base)
            .env("XDG_STATE_HOME", base.join("state"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("helper did not exit after rejecting unsafe settings");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        let events: Vec<serde_json::Value> = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert!(events.iter().any(|event| {
            event["event"] == "startup_failed"
                && event["code"] == "receiver_security_settings_invalid"
        }));
        assert!(
            !events.iter().any(|event| {
                event["event"] == "ready" || event["event"] == "incoming_pin_state"
            })
        );
        assert_eq!(fs::read_link(&settings).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), original);
        assert!(!state.join("identity.json").exists());
        fs::remove_dir_all(base).unwrap();
    }
}
