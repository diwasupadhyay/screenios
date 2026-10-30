use anyhow::{bail, Context, Result};
use ios_core::display::{
    DisplayServiceClient, MediaKind, MediaStreamOptions, RtpAssembler, RtpPacket,
};
use std::{
    net::Ipv6Addr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const MEDIA_PORT: u16 = 55000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Developer,
    QuickTime,
}

pub async fn capture_mode(
    stop: Arc<AtomicBool>,
    mode: Mode,
    consume: impl FnMut(&[u8]) -> Result<bool>,
) -> Result<()> {
    match mode {
        Mode::Developer => capture(stop, consume).await,
        Mode::QuickTime => crate::quicktime::capture(stop, consume).await,
    }
}

/// IPv6/UDP packet for RTCP feedback carried inside the USB CoreDevice tunnel.
fn udp_packet(source: Ipv6Addr, dest: Ipv6Addr, port: u16, payload: &[u8]) -> Vec<u8> {
    let len = payload.len() + 8;
    let mut p = vec![0u8; 40 + len];
    p[0] = 0x60;
    p[4..6].copy_from_slice(&(len as u16).to_be_bytes());
    p[6] = 17;
    p[7] = 64;
    p[8..24].copy_from_slice(&source.octets());
    p[24..40].copy_from_slice(&dest.octets());
    p[40..42].copy_from_slice(&MEDIA_PORT.to_be_bytes());
    p[42..44].copy_from_slice(&port.to_be_bytes());
    p[44..46].copy_from_slice(&(len as u16).to_be_bytes());
    p[48..].copy_from_slice(payload);
    let mut sum = len as u32 + 17;
    for chunk in p[8..40].chunks(2).chain(p[40..].chunks(2)) {
        sum += (u32::from(chunk[0]) << 8) | u32::from(*chunk.get(1).unwrap_or(&0));
    }
    while sum >> 16 != 0 {
        sum = (sum & 65535) + (sum >> 16);
    }
    let checksum = !(sum as u16);
    p[46..48].copy_from_slice(&if checksum == 0 { 65535 } else { checksum }.to_be_bytes());
    p
}

pub async fn capture(
    stop: Arc<AtomicBool>,
    mut consume: impl FnMut(&[u8]) -> Result<bool>,
) -> Result<()> {
    // This is the Apple service on this PC, never a remote usbmux endpoint.
    std::env::set_var("USBMUXD_SOCKET_ADDRESS", "127.0.0.1:27015");
    let devices = ios_core::list_devices().await.context("Apple USB service is unavailable. Install Apple Devices for Windows and reconnect the iPhone.")?;
    let usb: Vec<_> = devices
        .into_iter()
        .filter(|d| d.connection_type == "USB")
        .collect();
    if usb.len() != 1 {
        bail!("Connect one unlocked iPhone by USB and tap Trust.");
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        let pair=ios_core::PairRecord::load(&usb[0].udid)
            .map_err(|_| anyhow::anyhow!("Trust is not set up. Unlock your iPhone, open Apple Devices on this PC, and tap Trust on the phone. Enter the passcode on the phone, then retry."))?;
        let mux=ios_core::MuxClient::connect().await?;
        let stream=mux.connect_to_port(usb[0].device_id,ios_core::LOCKDOWN_PORT).await?;
        let mut lockdown=ios_core::LockdownClient::connect_with_stream(stream,&pair).await
            .context("Unlock your iPhone and accept Trust on the phone, then reconnect.")?;
        if let Ok(value)=lockdown.get_value(Some("com.apple.security.mac.amfi"),Some("DeveloperModeStatus")).await {
            if value.as_boolean()==Some(false) {
                bail!("Developer Mode is off. On your iPhone open Settings > Privacy & Security > Developer Mode, enable it, restart, unlock, and confirm Turn On. Then click Connect again.");
            }
        }
        Ok::<_,anyhow::Error>(())
    }).await.context("The iPhone did not respond. Unlock it and check for a Trust prompt.")??;
    let device = tokio::time::timeout(
        Duration::from_secs(25),
        ios_core::connect(&usb[0].udid, ios_core::ConnectOptions::default()),
    )
    .await.context("Connection timed out. Unlock your iPhone and confirm Developer Mode is enabled.")?
    .context("Could not open the developer USB connection. Check Trust and Developer Mode, then reconnect.")?;
    let tunnel = device.tunnel_handle().context("USB tunnel unavailable")?;
    let (mut packets, outbound) = tunnel
        .media_channels()
        .context("USB media channel unavailable")?;
    let source: Ipv6Addr = device
        .client_address()
        .context("Missing host endpoint")?
        .parse()?;
    let dest: Ipv6Addr = device
        .server_address()
        .context("Missing device endpoint")?
        .parse()?;
    let (xpc, metadata) = device
        .connect_xpc_service_with_metadata(ios_core::display::SERVICE_NAME)
        .await?;
    let mut display = DisplayServiceClient::from_resolved_metadata(xpc, &metadata)?;
    let options = MediaStreamOptions {
        receiver_ip: source.to_string(),
        sender_ip: dest.to_string(),
        receiver_port: MEDIA_PORT,
        ..Default::default()
    };
    let stream = display.start_video_stream(&options).await?;
    let mut assembler = RtpAssembler::new(MediaKind::Video, 8 * 1024 * 1024, 4096);
    let mut feedback = tokio::time::interval(Duration::from_secs(1));
    let mut highest: u32 = 0;
    let mut seen = false;
    let mut last_unit = Instant::now();
    let mut need_key = false;
    let result: Result<()> = async {
        while !stop.load(Ordering::Relaxed) {
            tokio::select! {
                _ = feedback.tick() => {
                    if !tunnel.is_alive() { bail!("USB cable disconnected. Reconnect and try again."); }
                    if last_unit.elapsed() > Duration::from_secs(25) { bail!("Video paused too long. Unlock your phone and reconnect."); }
                    if let (Some(port), Some(local), Some(remote)) = (stream.sender_port, stream.local_ssrc, stream.remote_ssrc) {
                        let mut rr = vec![0x81, 201, 0, 7];
                        rr.extend(local.to_be_bytes()); rr.extend(remote.to_be_bytes());
                        rr.extend([0u8;4]); rr.extend(highest.to_be_bytes()); rr.extend([0u8;12]);
                        outbound.send(udp_packet(source, dest, port, &rr)).await?;
                        if need_key || last_unit.elapsed() > Duration::from_secs(8) {
                            let mut pli = vec![0x81,206,0,2];
                            pli.extend(local.to_be_bytes()); pli.extend(remote.to_be_bytes());
                            outbound.send(udp_packet(source,dest,port,&pli)).await?;
                        }
                    }
                }
                packet = packets.recv() => {
                    let packet = match packet {
                        Ok(p) => p,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => { need_key = true; continue; }
                        Err(_) => bail!("USB stream closed"),
                    };
                    if packet.len() < 60 || packet[24..40] != source.octets() || packet[8..24] != dest.octets()
                        || u16::from_be_bytes([packet[42],packet[43]]) != MEDIA_PORT { continue; }
                    let data = &packet[48..];
                    if (64..=95).contains(&(data[1]&127)) { continue; }
                    let Some(rtp) = RtpPacket::parse(data)? else { continue; };
                    if seen {
                        let delta = rtp.sequence.wrapping_sub(highest as u16);
                        if delta > 1 && delta < 32768 { need_key = true; }
                        if delta < 32768 { highest = highest.wrapping_add(delta as u32); }
                    } else { highest = rtp.sequence as u32; seen = true; }
                    if let Some(unit) = assembler.push(rtp)? {
                        let key = is_key(&unit.data);
                        if need_key && !key { continue; }
                        if key { need_key = false; }
                        if !consume(&unit.data)? { need_key = true; }
                        last_unit = Instant::now();
                    }
                }
            }
        }
        Ok(())
    }.await;
    let cleanup = tokio::time::timeout(Duration::from_secs(5), async {
        let (xpc, metadata) = device
            .connect_xpc_service_with_metadata(ios_core::display::SERVICE_NAME)
            .await?;
        let mut control = DisplayServiceClient::from_resolved_metadata(xpc, &metadata)?;
        control.stop_media_stream(stream.client_session_id).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await;
    if result.is_ok() {
        cleanup??;
    }
    result
}

pub fn is_key(data: &[u8]) -> bool {
    data.windows(5)
        .any(|w| w[..4] == [0, 0, 0, 1] && (16..=21).contains(&((w[4] >> 1) & 63)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn udp_checksum_and_addresses() {
        let a = "fd00::1".parse().unwrap();
        let b = "fd00::2".parse().unwrap();
        let p = udp_packet(a, b, 1234, &[1, 2, 3]);
        assert_eq!(&p[8..24], &a.octets());
        let mut sum = 11u32 + 17;
        for c in p[8..].chunks(2) {
            sum += ((c[0] as u32) << 8) | *c.get(1).unwrap_or(&0) as u32;
        }
        while sum >> 16 != 0 {
            sum = (sum & 65535) + (sum >> 16);
        }
        assert_eq!(sum, 65535);
    }
}
