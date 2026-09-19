use std::{fs, process::Command};

#[test]
fn loads_dotenv_next_to_config_without_overriding_process_environment() {
    let root = std::env::temp_dir().join(format!("clipfarmer-dotenv-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let config = root.join("clipfarmer.toml");
    fs::write(&config, include_str!("../clipfarmer.toml.example")).unwrap();
    fs::write(
        root.join(".env"),
        "TWITCH_ACCESS_TOKEN=dotenv-token\nTWITCH_CLIENT_ID=dotenv-client\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_clipfarmer"))
        .env_clear()
        .env("TWITCH_ACCESS_TOKEN", "")
        .args(["--config"])
        .arg(&config)
        .args(["auth", "twitch"])
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();

    let _ = fs::remove_dir_all(&root);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("TWITCH_ACCESS_TOKEN: missing"));
    assert!(stdout.contains("TWITCH_CLIENT_ID: configured"));
}
