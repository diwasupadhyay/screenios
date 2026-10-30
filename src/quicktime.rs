use anyhow::{bail, Context, Result};
use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::AsyncReadExt;

/// A separate capture-only helper speaks Apple's raw USB QuickTime protocol.
/// Never install drivers or silently fall back to Developer Mode.
pub async fn capture(
    stop: Arc<AtomicBool>,
    mut consume: impl FnMut(&[u8]) -> Result<bool>,
) -> Result<()> {
    let helper = std::env::current_exe()?.with_file_name("ScreenIOS.UsbCapture.exe");
    let mut child = tokio::process::Command::new(helper)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context(
            "USB capture helper is missing. Rebuild or restore the complete ScreenIOS folder.",
        )?;
    let stdin = child.stdin.take();
    let mut stdout = child.stdout.take().context("Capture pipe unavailable")?;
    let stderr = child
        .stderr
        .take()
        .context("Capture diagnostics unavailable")?;
    let errors = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.take(4096).read_to_string(&mut text).await;
        text
    });
    let result: Result<()> = {
        let stream = async {
            let mut bytes = Vec::new();
            loop {
                let length = tokio::time::timeout(Duration::from_secs(25), stdout.read_u32_le())
                    .await
                    .context(
                        "No video received. Unlock your iPhone and accept Trust if prompted.",
                    )?? as usize;
                if length == 0 || length > 8 * 1024 * 1024 {
                    bail!("Invalid USB video packet size");
                }
                bytes.resize(length, 0);
                tokio::time::timeout(Duration::from_secs(5), stdout.read_exact(&mut bytes))
                    .await??;
                // Every helper packet includes SPS/PPS; recovery skips to the next IDR.
                consume(&bytes)?;
            }
        };
        tokio::pin!(stream);
        tokio::select! {
            result=&mut stream => result,
            _=async {while !stop.load(Ordering::Relaxed){tokio::time::sleep(Duration::from_millis(100)).await;}} => Ok(()),
        }
    };
    drop(stdin);
    if tokio::time::timeout(Duration::from_secs(3), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
    }
    let message = errors.await.unwrap_or_default();
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    if !stop.load(Ordering::Relaxed) && !message.trim().is_empty() {
        bail!("{}", message.trim());
    }
    result
}

pub fn is_key(data: &[u8]) -> bool {
    data.windows(5)
        .any(|w| w[..4] == [0, 0, 0, 1] && w[4] & 31 == 5)
}

#[cfg(test)]
mod tests {
    #[test]
    fn recovery_requires_idr_not_parameter_sets() {
        assert!(!super::is_key(&[0, 0, 0, 1, 0x67, 0, 0, 0, 1, 0x68]));
        assert!(super::is_key(&[0, 0, 0, 1, 0x65, 0x80]));
        assert!(!super::is_key(&[0, 0, 0, 1, 0x41, 0x80]));
    }
}
