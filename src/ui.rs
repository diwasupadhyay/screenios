use crate::{
    decoder::{Decoder, VideoFrame},
    transport,
};
use eframe::egui::{self, Color32, RichText, Vec2, ViewportCommand};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
unsafe extern "C" {
    fn window_aspect(handle: *mut std::ffi::c_void, ratio: f64, header: i32, minimum: i32);
}
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const MINT: Color32 = Color32::from_rgb(116, 223, 196);
const BG: Color32 = Color32::from_rgb(15, 19, 27);

#[derive(Default)]
struct Shared {
    latest: Option<VideoFrame>,
    frames: u64,
    first_frame: Option<f64>,
    error: Option<String>,
    done: bool,
}

pub struct App {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    target_w: Arc<AtomicU32>,
    target_h: Arc<AtomicU32>,
    worker: Option<thread::JoinHandle<()>>,
    texture: Option<egui::TextureHandle>,
    logo: egui::TextureHandle,
    mode: transport::Mode,
    driver_present: bool,
    driver_job: Option<std::sync::mpsc::Receiver<anyhow::Result<String>>>,
    driver_message: Option<String>,
    connecting: bool,
    stopping: bool,
    fullscreen: bool,
    close_pending: bool,
    hardware: bool,
    source: [usize; 2],
    error: Option<String>,
    smoke: bool,
    started: Instant,
    smoke_phase: usize,
    smoke_frames: u64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, smoke: bool) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = BG;
        style.visuals.selection.bg_fill = MINT;
        style.spacing.button_padding = Vec2::new(10., 4.);
        cc.egui_ctx.set_style(style);
        let icon = eframe::icon_data::from_png_bytes(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/screenios.png"
        )))
        .expect("Embedded logo");
        let logo = cc.egui_ctx.load_texture(
            "project-logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        Self {
            shared: Arc::new(Mutex::new(Shared::default())),
            stop: Arc::new(AtomicBool::new(false)),
            target_w: Arc::new(AtomicU32::new(720)),
            target_h: Arc::new(AtomicU32::new(1280)),
            worker: None,
            texture: None,
            logo,
            driver_present: crate::driver::installed(),
            driver_job: None,
            driver_message: None,
            mode: if std::env::args().any(|a| a == "--quicktime") {
                transport::Mode::QuickTime
            } else {
                transport::Mode::Developer
            },
            connecting: false,
            stopping: false,
            fullscreen: false,
            close_pending: false,
            hardware: false,
            source: [1, 1],
            error: None,
            smoke,
            started: Instant::now(),
            smoke_phase: 0,
            smoke_frames: 0,
        }
    }

    fn connect(&mut self, ctx: &egui::Context) {
        if self.worker.is_some() || self.driver_job.is_some() {
            return;
        }
        self.shared = Arc::new(Mutex::new(Shared::default()));
        self.stop = Arc::new(AtomicBool::new(false));
        self.connecting = true;
        self.error = None;
        self.target_w.store(720, Ordering::Relaxed);
        self.target_h.store(1280, Ordering::Relaxed);
        let mode = self.mode;
        let (shared, stop, w, h, ctx) = (
            self.shared.clone(),
            self.stop.clone(),
            self.target_w.clone(),
            self.target_h.clone(),
            ctx.clone(),
        );
        self.worker = Some(thread::spawn(move || {
            let begin = Instant::now();
            let result = (|| -> anyhow::Result<()> {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()?;
                let h264 = mode == transport::Mode::QuickTime;
                let mut decoder =
                    Decoder::new(false, h264).or_else(|_| Decoder::new(true, h264))?;
                let mut decoder_recovery = false;
                let mut software_retry = false;
                rt.block_on(async {
                let session = transport::capture_mode(stop.clone(),mode,|data| {
                    if decoder_recovery && !(if h264 {crate::quicktime::is_key(data)} else {transport::is_key(data)}){return Ok(false);}
                    let frame=match decoder.decode(data,[w.load(Ordering::Relaxed),h.load(Ordering::Relaxed)]) {
                        Ok(f)=>f,
                        Err(_) if !software_retry => {
                            // A failed hardware codec is replaced once with the software decoder.
                            decoder=Decoder::new(true,h264)?; decoder_recovery=true; software_retry=true;
                            return Ok(false);
                        },
                        Err(e)=>return Err(e),
                    };
                    if let Some(frame)=frame {
                        decoder_recovery=false;
                        let mut state=shared.lock().unwrap();
                        state.latest=Some(frame);state.frames+=1;
                        state.first_frame.get_or_insert(begin.elapsed().as_secs_f64());
                        drop(state);ctx.request_repaint();
                    }
                    Ok(true)
                });
                tokio::pin!(session);
                tokio::select! {
                    result = &mut session => result,
                    _ = async { while !stop.load(Ordering::Relaxed) {tokio::time::sleep(Duration::from_millis(100)).await;} } => {
                        // Give stream shutdown time to finish; a stalled setup cannot trap the window.
                        tokio::time::timeout(Duration::from_secs(7), &mut session).await
                            .map_err(|_| anyhow::anyhow!("USB connection closed after shutdown timeout"))?
                    }
                }
                })
            })();
            let mut state = shared.lock().unwrap();
            if let Err(e) = result {
                if !stop.load(Ordering::Relaxed) {
                    state.error = Some(e.to_string());
                }
            }
            state.done = true;
            drop(state);
            ctx.request_repaint();
        }));
    }

    fn disconnect(&mut self) {
        if self.worker.is_some() {
            self.stopping = true;
            self.stop.store(true, Ordering::Relaxed);
        }
    }
    fn fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }
    fn update_video(&mut self, ctx: &egui::Context) {
        let (frame, done, error) = {
            let mut s = self.shared.lock().unwrap();
            (s.latest.take(), s.done, s.error.take())
        };
        if let Some(frame) = frame {
            let first = self.texture.is_none();
            let old_ratio = self.source[0] as f32 / self.source[1] as f32;
            self.source = frame.source;
            let ratio = self.source[0] as f32 / self.source[1] as f32;
            self.hardware = frame.hardware;
            if let Some(texture) = self.texture.as_mut() {
                if texture.size() == frame.image.size {
                    // Update existing GPU storage instead of reallocating it at video rate.
                    texture.set_partial([0, 0], frame.image, egui::TextureOptions::LINEAR);
                } else {
                    texture.set(frame.image, egui::TextureOptions::LINEAR);
                }
            } else {
                self.texture = Some(ctx.load_texture(
                    "iphone-screen",
                    frame.image,
                    egui::TextureOptions::LINEAR,
                ));
            }
            if first || (!self.fullscreen && (ratio - old_ratio).abs() > 0.01) {
                self.connecting = false;
                let monitor = ctx
                    .input(|i| i.viewport().monitor_size)
                    .unwrap_or(Vec2::new(1280., 860.));
                let height = (monitor.y - 100.).min(820.).min((monitor.x - 100.) / ratio);
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(
                    220.,
                    220. / ratio + 32.,
                )));
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
                    height * ratio,
                    height + 32.,
                )));
            }
        }
        if done && self.worker.is_some() {
            if self.smoke {
                crate::log(&format!("capture finished: {:?}", error));
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            self.error = error;
            self.texture = None;
            self.connecting = false;
            self.stopping = false;
            self.fullscreen = false;
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(420., 480.)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(420., 480.)));
            if self.close_pending {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn connection_card(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(24))
            .show(ctx, |ui| {
                self.controls(ui, ctx, false);
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    ui.image((self.logo.id(), Vec2::splat(48.)));
                    ui.add_space(8.);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ScreenIOS").size(24.).strong());
                        ui.label(RichText::new("Your iPhone. On your desktop.").color(Color32::from_gray(155)));
                    });
                });
                ui.add_space(22.);
                ui.add_enabled_ui(!self.connecting && !self.stopping, |ui| {
                    for (mode, label) in [(transport::Mode::QuickTime,"Without Developer Mode"),(transport::Mode::Developer,"Developer Mode")] {
                        let selected = self.mode==mode;
                        let button=egui::Button::new(RichText::new(label).size(14.))
                            .fill(if selected {Color32::from_rgb(34,49,51)} else {Color32::from_rgb(24,28,35)})
                            .stroke(egui::Stroke::new(1_f32,if selected {MINT} else {Color32::from_gray(48)}));
                        if ui.add_sized([ui.available_width(),36.],button).clicked(){self.mode=mode;self.error=None;}
                        ui.add_space(4.);
                    }
                });
                ui.add_space(8.);
                ui.label("Unlock your iPhone and connect it by USB. Tap Trust on the phone if asked, then enter its passcode there.");
                ui.add_space(8.);
                ui.label(RichText::new(if self.mode==transport::Mode::Developer {
                    "Enable Settings > Privacy & Security > Developer Mode. Restart and confirm Turn On when iOS asks."
                } else {
                    "Experimental QuickTime USB capture. Requires UsbDk on Windows. Not yet verified with Developer Mode off."
                }).size(12.).color(Color32::from_gray(160)));
                ui.add_space(16.);
                if self.mode == transport::Mode::QuickTime {
                    if self.driver_present {
                        ui.label(RichText::new("USB capture driver installed").size(12.).color(MINT));
                    } else if ui.add_enabled(self.driver_job.is_none() && self.worker.is_none(), egui::Button::new("Install USB driver")).clicked() {
                        let (sender,receiver)=std::sync::mpsc::channel();
                        self.driver_job=Some(receiver);
                        self.driver_message=None;
                        let ctx=ctx.clone();
                        thread::spawn(move || {let _=sender.send(crate::driver::install());ctx.request_repaint();});
                    }
                    if self.driver_job.is_some() {ui.label("Installing driver — approve the Windows administrator prompt.");}
                    if let Some(message)=&self.driver_message {ui.label(RichText::new(message).size(12.));}
                    ui.add_space(8.);
                }
                if self.connecting || self.stopping {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if self.stopping {
                            "Disconnecting…"
                        } else {
                            "Connecting…"
                        });
                    });
                    ui.add_space(10.);
                    if !self.stopping && ui.button("Cancel").clicked() {
                        self.disconnect();
                    }
                    ctx.request_repaint_after(Duration::from_millis(100));
                } else {
                    let button =
                        egui::Button::new(RichText::new("Connect").size(15.).strong().color(BG))
                            .fill(MINT);
                    if ui.add_enabled_ui(self.driver_job.is_none(), |ui| ui.add_sized([ui.available_width(), 38.], button)).inner.clicked() {
                        self.connect(ctx);
                    }
                }
                ui.add_space(12.);
                if let Some(error) = &self.error {
                    egui::ScrollArea::vertical().max_height(100.).show(ui, |ui| {ui.label(
                        RichText::new(error)
                            .size(12.)
                            .color(Color32::from_rgb(255, 170, 155)),
                    );});
                } else {
                    ui.label(
                        RichText::new("USB only · View only")
                            .size(12.)
                            .color(Color32::from_gray(145)),
                    );
                }
            });
    }

    fn controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, video: bool) {
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 26.),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if icon_button(ui, 2, "Close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                if icon_button(ui, 0, "Minimize").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
                if video {
                    if icon_button(ui, 1, "Fullscreen (F11)").clicked() {
                        self.fullscreen(ctx);
                    }
                    if ui
                        .add_enabled(
                            !self.stopping,
                            egui::Button::new(if self.stopping {
                                "Stopping…"
                            } else {
                                "Disconnect"
                            })
                            .frame(false),
                        )
                        .clicked()
                    {
                        self.disconnect();
                    }
                }
                let drag = ui.allocate_response(
                    Vec2::new(ui.available_width().max(0.), 26.),
                    egui::Sense::click_and_drag(),
                );
                if drag.drag_started() && !self.fullscreen {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if video && drag.double_clicked() {
                    self.fullscreen(ctx);
                }
            },
        );
    }

    fn resize_edges(&self, ctx: &egui::Context) {
        if self.fullscreen {
            return;
        }
        let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let r = ctx.content_rect();
        let left = p.x - r.left() < 5.;
        let right = r.right() - p.x < 5.;
        let top = p.y - r.top() < 5.;
        let bottom = r.bottom() - p.y < 5.;
        use egui::{CursorIcon as C, ResizeDirection as D};
        let hit = if (left && p.y - r.top() < 14.) || (top && p.x - r.left() < 14.) {
            Some((D::NorthWest, C::ResizeNwSe))
        } else if (right && p.y - r.top() < 14.) || (top && r.right() - p.x < 14.) {
            Some((D::NorthEast, C::ResizeNeSw))
        } else if (left && r.bottom() - p.y < 14.) || (bottom && p.x - r.left() < 14.) {
            Some((D::SouthWest, C::ResizeNeSw))
        } else if (right && r.bottom() - p.y < 14.) || (bottom && r.right() - p.x < 14.) {
            Some((D::SouthEast, C::ResizeNwSe))
        } else if left {
            Some((D::West, C::ResizeHorizontal))
        } else if right {
            Some((D::East, C::ResizeHorizontal))
        } else if top {
            Some((D::North, C::ResizeVertical))
        } else if bottom {
            Some((D::South, C::ResizeVertical))
        } else {
            None
        };
        if let Some((direction, cursor)) = hit {
            ctx.set_cursor_icon(cursor);
            if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
            }
        }
    }
    fn floating_screen(&mut self, ctx: &egui::Context) {
        // The only chrome is this slim strip; video starts below it.
        egui::TopBottomPanel::top("video-header")
            .exact_height(32.)
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(6, 3)),
            )
            .show(ctx, |ui| {
                self.controls(ui, ctx, true);
            });
        let texture = self.texture.as_ref().unwrap().clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let ratio = self.source[0] as f32 / self.source[1] as f32;
                let size = if rect.width() / rect.height() > ratio {
                    Vec2::new(rect.height() * ratio, rect.height())
                } else {
                    Vec2::new(rect.width(), rect.width() / ratio)
                };
                let pixels = ctx.pixels_per_point();
                // Quantize conversion dimensions to avoid reallocating the decoder's
                // scale context and GPU texture for every one-pixel resize movement.
                self.target_w.store(
                    ((size.x * pixels).ceil() as u32).div_ceil(32) * 32,
                    Ordering::Relaxed,
                );
                self.target_h.store(
                    ((size.y * pixels).ceil() as u32).div_ceil(32) * 32,
                    Ordering::Relaxed,
                );
                if ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
                    self.target_w.store(0, Ordering::Relaxed);
                    self.target_h.store(0, Ordering::Relaxed);
                }
                let dest = if self.fullscreen {
                    egui::Rect::from_center_size(rect.center(), size)
                } else {
                    // WM_SIZING already preserves the video ratio. Fill the complete
                    // region so fractional DPI rounding cannot leave a one-pixel bar.
                    rect
                };
                ui.painter().image(
                    texture.id(),
                    dest,
                    egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
                if ui
                    .interact(rect, ui.id().with("screen"), egui::Sense::click())
                    .double_clicked()
                {
                    self.fullscreen(ctx);
                }
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(job) = &self.driver_job {
            match job.try_recv() {
                Ok(result) => {
                    self.driver_message = Some(match result {
                        Ok(message) => message,
                        Err(e) => e.to_string(),
                    });
                    self.driver_present = crate::driver::installed();
                    self.driver_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.driver_message =
                        Some("Driver setup ended unexpectedly. Retry installation.".into());
                    self.driver_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(200))
                }
            }
        }
        self.update_video(ctx);
        if let Ok(handle) = frame.window_handle() {
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                let ratio = if self.texture.is_some() && !self.fullscreen {
                    self.source[0] as f64 / self.source[1] as f64
                } else {
                    0.
                };
                unsafe {
                    window_aspect(
                        handle.hwnd.get() as *mut _,
                        ratio,
                        (32. * ctx.pixels_per_point()).round() as i32,
                        (220. * ctx.pixels_per_point()).round() as i32,
                    );
                }
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.worker.is_some() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.close_pending = true;
            self.disconnect();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) && self.texture.is_some() {
            self.fullscreen(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.fullscreen {
            self.fullscreen(ctx);
        }
        if self.texture.is_some() {
            self.floating_screen(ctx);
        } else {
            self.connection_card(ctx);
        }
        self.resize_edges(ctx);
        if self.close_pending && self.worker.is_none() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        if self.smoke {
            let seconds = self.started.elapsed().as_secs_f32();
            match self.smoke_phase {
                0 => {
                    self.connect(ctx);
                    self.smoke_phase = 1;
                }
                1 if seconds > 10. => {
                    self.fullscreen(ctx);
                    self.smoke_phase = 2;
                }
                2 if seconds > 12. => {
                    self.fullscreen(ctx);
                    self.smoke_phase = 3;
                }
                3 if seconds > 17. => {
                    let s = self.shared.lock().unwrap();
                    self.smoke_frames = s.frames;
                    crate::log(&format!(
                        "cycle1 frames={} first={:?} hardware={} source={:?} rendered={}",
                        s.frames,
                        s.first_frame,
                        self.hardware,
                        self.source,
                        self.texture.is_some()
                    ));
                    drop(s);
                    self.disconnect();
                    self.smoke_phase = 4;
                }
                4 if self.worker.is_none() => {
                    self.connect(ctx);
                    self.smoke_phase = 5;
                }
                5 if seconds > 32. => {
                    let s = self.shared.lock().unwrap();
                    crate::log(&format!(
                        "cycle2 frames={} first={:?} hardware={} rendered={}",
                        s.frames,
                        s.first_frame,
                        self.hardware,
                        self.texture.is_some()
                    ));
                    drop(s);
                    self.close_pending = true;
                    self.disconnect();
                    self.smoke_phase = 6;
                }
                _ => {}
            }
            if seconds > 50. {
                self.disconnect();
                self.close_pending = true;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn icon_button(ui: &mut egui::Ui, kind: u8, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(28., 26.), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 3., Color32::from_gray(48));
    }
    let c = rect.center();
    let stroke = egui::Stroke::new(1.2_f32, Color32::from_gray(210));
    match kind {
        0 => {
            ui.painter()
                .line_segment([c + Vec2::new(-5., 0.), c + Vec2::new(5., 0.)], stroke);
        }
        1 => {
            ui.painter().rect_stroke(
                egui::Rect::from_center_size(c, Vec2::new(10., 9.)),
                0.,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        _ => {
            ui.painter()
                .line_segment([c - Vec2::splat(4.), c + Vec2::splat(4.)], stroke);
            ui.painter()
                .line_segment([c + Vec2::new(-4., 4.), c + Vec2::new(4., -4.)], stroke);
        }
    }
    response.on_hover_text(label)
}

#[cfg(test)]
mod sizing_tests {
    unsafe extern "C" {
        fn window_constrain_test(
            coords: *mut i32,
            edge: u32,
            ratio: f64,
            header: i32,
            minimum: i32,
        );
    }
    #[test]
    fn all_resize_edges_preserve_phone_ratio_and_anchor() {
        for edge in 1..=8 {
            let mut r = [100, 80, 580, 800];
            unsafe {
                window_constrain_test(r.as_mut_ptr(), edge, 1184. / 2576., 40, 275);
            }
            let w = r[2] - r[0];
            let h = r[3] - r[1] - 40;
            assert!((h as f64 - w as f64 / (1184. / 2576.)).abs() <= 0.51);
            assert!(w >= 275);
            if [1, 4, 7].contains(&edge) {
                assert_eq!(r[2], 580);
            } else {
                assert_eq!(r[0], 100);
            }
            if [3, 4, 5].contains(&edge) {
                assert_eq!(r[3], 800);
            } else {
                assert_eq!(r[1], 80);
            }
        }
    }
    #[test]
    fn minimum_width_preserves_ratio() {
        let mut r = [0, 0, 20, 40];
        unsafe {
            window_constrain_test(r.as_mut_ptr(), 8, 0.5, 32, 220);
        }
        assert_eq!(r, [0, 0, 220, 472]);
    }
}
