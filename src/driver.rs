use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::os::windows::ffi::OsStrExt;
unsafe extern "C" {
    fn driver_installed() -> i32;
    fn driver_install(path: *const u16) -> u32;
}
pub fn installed() -> bool {
    unsafe { driver_installed() != 0 }
}
pub fn install() -> Result<String> {
    let file = std::env::current_exe()?
        .parent()
        .context("App folder unavailable")?
        .join("drivers/UsbDk_1.0.22_x64.msi");
    let bytes = std::fs::read(&file).context(
        "The bundled USB driver installer is missing. Restore the complete ScreenIOS folder.",
    )?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != "91f6f695e1e13c656024e6d3b55620bf08d8835ef05ee0496935ba6bb62466a5" {
        bail!("Driver installer verification failed. Rebuild or restore the original ScreenIOS package.");
    }
    drop(bytes);
    let path: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
    match unsafe{driver_install(path.as_ptr())} {
        0 if installed()=>Ok("Driver installed. Reconnect your iPhone, then click Connect.".into()),
        3010=>Ok("Driver installed. Restart Windows before connecting.".into()),
        1223|1602=>bail!("Driver installation was cancelled."),
        code=>bail!("Driver setup did not complete (code {code}). Retry the installation and approve the Windows administrator prompt."),
    }
}
