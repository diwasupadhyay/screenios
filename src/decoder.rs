use anyhow::{bail, Result};
use std::ffi::c_void;

unsafe extern "C" {
    fn video_create(software: i32) -> *mut c_void;
    fn video_decode(d: *mut c_void, data: *const u8, len: i32, w: i32, h: i32) -> i32;
    fn video_pixels(
        d: *mut c_void,
        w: *mut i32,
        h: *mut i32,
        sw: *mut i32,
        sh: *mut i32,
        hardware: *mut i32,
    ) -> *const u8;
    fn video_destroy(d: *mut c_void);
}
pub struct VideoFrame {
    pub image: eframe::egui::ColorImage,
    pub source: [usize; 2],
    pub hardware: bool,
}
pub struct Decoder(*mut c_void);
impl Decoder {
    pub fn new(software: bool) -> Result<Self> {
        let p = unsafe { video_create(software as i32) };
        if p.is_null() {
            bail!("Video decoder could not load. Keep all four FFmpeg DLLs beside ScreenIOS.exe.");
        }
        Ok(Self(p))
    }
    pub fn decode(&mut self, data: &[u8], target: [u32; 2]) -> Result<Option<VideoFrame>> {
        let result = unsafe {
            video_decode(
                self.0,
                data.as_ptr(),
                data.len() as i32,
                target[0] as i32,
                target[1] as i32,
            )
        };
        if result < 0 {
            bail!("Video decoder returned {result}");
        }
        if result == 0 {
            return Ok(None);
        }
        let (mut w, mut h, mut sw, mut sh, mut hw) = (0, 0, 0, 0, 0);
        let ptr = unsafe { video_pixels(self.0, &mut w, &mut h, &mut sw, &mut sh, &mut hw) };
        let bytes = unsafe { std::slice::from_raw_parts(ptr, w as usize * h as usize * 4) };
        Ok(Some(VideoFrame {
            image: eframe::egui::ColorImage::from_rgba_unmultiplied(
                [w as usize, h as usize],
                bytes,
            ),
            source: [sw as usize, sh as usize],
            hardware: hw != 0,
        }))
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { video_destroy(self.0) }
    }
}
