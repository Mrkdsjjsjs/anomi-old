use anyhow::Result;

/// List native audio devices without creating or publishing a microphone track.
pub fn audio_devices() -> Result<()> {
    let audio = livekit::PlatformAudio::new().map_err(|_| {
        anyhow::anyhow!("audio devices unavailable; check audio service and permissions")
    })?;
    println!("Microphones:");
    for device in audio.recording_devices() {
        println!("{}", device.name);
    }
    println!("Speakers:");
    for device in audio.playout_devices() {
        println!("{}", device.name);
    }
    println!("Cameras:");
    for device in crate::camera::list() {
        println!("{}", device.name);
    }
    println!("No microphone track was created or published, and no camera filmed.");
    Ok(())
}
