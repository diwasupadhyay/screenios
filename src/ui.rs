use crate::{decoder::{Decoder, VideoFrame}, transport};
use eframe::egui::{self, Color32, RichText, Vec2, ViewportCommand};
use std::{sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU32, Ordering}}, thread, time::{Duration, Instant}};

const MINT: Color32 = Color32::from_rgb(116,223,196);
const BG: Color32 = Color32::from_rgb(15,19,27);

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
    connecting: bool,
    stopping: bool,
    fullscreen: bool,
    pinned: bool,
    close_pending: bool,
    hardware: bool,
    source: [usize;2],
    error: Option<String>,
    hover_until: Instant,
    smoke: bool,
    started: Instant,
    smoke_phase: usize,
    smoke_frames: u64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, smoke: bool)->Self {
        let mut style=(*cc.egui_ctx.style()).clone();
        style.visuals=egui::Visuals::dark();
        style.visuals.panel_fill=BG;
        style.visuals.window_fill=BG;
        style.visuals.selection.bg_fill=MINT;
        style.spacing.button_padding=Vec2::new(18.,10.);
        cc.egui_ctx.set_style(style);
        Self {shared:Arc::new(Mutex::new(Shared::default())),stop:Arc::new(AtomicBool::new(false)),
            target_w:Arc::new(AtomicU32::new(720)),target_h:Arc::new(AtomicU32::new(1280)),
            worker:None,texture:None,connecting:false,stopping:false,fullscreen:false,pinned:false,
            close_pending:false,hardware:false,source:[1,1],error:None,hover_until:Instant::now(),
            smoke,started:Instant::now(),smoke_phase:0,smoke_frames:0}
    }

    fn connect(&mut self,ctx:&egui::Context) {
        if self.worker.is_some(){return;}
        self.shared=Arc::new(Mutex::new(Shared::default()));
        self.stop=Arc::new(AtomicBool::new(false)); self.connecting=true;self.error=None;
        let (shared,stop,w,h,ctx)=(self.shared.clone(),self.stop.clone(),self.target_w.clone(),self.target_h.clone(),ctx.clone());
        self.worker=Some(thread::spawn(move || {
            let begin=Instant::now();
            let result=(|| -> anyhow::Result<()> {
                let rt=tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
                let mut decoder=Decoder::new(false).or_else(|_| Decoder::new(true))?;
                let mut decoder_recovery=false;
                rt.block_on(transport::capture(stop.clone(),|data| {
                    if decoder_recovery && !transport::is_key(data){return Ok(());}
                    let frame=match decoder.decode(data,[w.load(Ordering::Relaxed),h.load(Ordering::Relaxed)]) {
                        Ok(f)=>f,
                        Err(_) if !decoder_recovery => {
                            // A failed hardware codec is replaced once with the software decoder.
                            decoder=Decoder::new(true)?; decoder_recovery=true;
                            return Ok(());
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
                    Ok(())
                }))
            })();
            let mut state=shared.lock().unwrap();
            if let Err(e)=result { if !stop.load(Ordering::Relaxed){state.error=Some(e.to_string());} }
            state.done=true;drop(state);ctx.request_repaint();
        }));
    }

    fn disconnect(&mut self){
        if self.worker.is_some(){self.stopping=true;self.stop.store(true,Ordering::Relaxed);}
    }
    fn fullscreen(&mut self,ctx:&egui::Context){
        self.fullscreen=!self.fullscreen;ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }
    fn update_video(&mut self,ctx:&egui::Context){
        let (frame,done,error)={let mut s=self.shared.lock().unwrap();(s.latest.take(),s.done,s.error.take())};
        if let Some(frame)=frame {
            let first=self.texture.is_none();
            self.source=frame.source;self.hardware=frame.hardware;
            if let Some(texture)=self.texture.as_mut(){texture.set(frame.image,egui::TextureOptions::LINEAR);}
            else{self.texture=Some(ctx.load_texture("iphone-screen",frame.image,egui::TextureOptions::LINEAR));}
            if first {
                self.connecting=false;self.hover_until=Instant::now()+Duration::from_secs(3);
                let height=ctx.input(|i| i.viewport().monitor_size.map(|s|s.y-100.).unwrap_or(760.)).min(820.);
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(height*self.source[0] as f32/self.source[1] as f32,height)));
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(220.,300.)));
            }
        }
        if done && self.worker.is_some(){
            if self.smoke {crate::log(&format!("capture finished: {:?}", error));}
            if let Some(worker)=self.worker.take(){let _=worker.join();}
            self.error=error;self.texture=None;self.connecting=false;self.stopping=false;
            self.fullscreen=false;ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(380.,320.)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(400.,360.)));
            if self.close_pending {ctx.send_viewport_cmd(ViewportCommand::Close);}
        }
    }

    fn titlebar(&mut self,ui:&mut egui::Ui,ctx:&egui::Context){
        ui.horizontal(|ui|{
            ui.label(RichText::new("ScreenIOS").size(14.).strong());
            let available=(ui.available_width()-64.).max(0.);
            let drag=ui.allocate_response(Vec2::new(available,28.),egui::Sense::drag());
            if drag.drag_started(){ctx.send_viewport_cmd(ViewportCommand::StartDrag);}
            if ui.small_button("—").on_hover_text("Minimize").clicked(){ctx.send_viewport_cmd(ViewportCommand::Minimized(true));}
            if ui.small_button("×").on_hover_text("Close").clicked(){ctx.send_viewport_cmd(ViewportCommand::Close);}
        });
    }

    fn connection_card(&mut self,ctx:&egui::Context){
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(22)).show(ctx,|ui|{
            self.titlebar(ui,ctx);
            ui.add_space(26.);
            ui.label(RichText::new("Your screen,\nunbound.").size(32.).strong());
            ui.add_space(12.);
            ui.label(RichText::new("Plug in your iPhone. Unlock it.\nYour screen opens in its own floating window.").size(14.).color(Color32::from_gray(166)));
            ui.add_space(22.);
            if self.connecting || self.stopping {
                ui.horizontal(|ui|{ui.spinner();ui.label(if self.stopping{"Disconnecting…"}else{"Connecting over USB…"});});
                if !self.stopping && ui.small_button("Cancel").clicked(){self.disconnect();}
                ctx.request_repaint_after(Duration::from_millis(100));
            } else {
                let button=egui::Button::new(RichText::new("Connect iPhone").size(16.).strong().color(BG)).fill(MINT);
                if ui.add_sized([ui.available_width(),46.],button).clicked(){self.connect(ctx);}
            }
            ui.add_space(12.);
            if let Some(error)=&self.error {ui.label(RichText::new(error).size(12.).color(Color32::from_rgb(255,170,155)));}
            else {ui.label(RichText::new("USB ONLY   ·   VIEW ONLY   ·   NO CLOUD").size(10.).color(Color32::from_gray(125)));}
        });
    }

    fn floating_screen(&mut self,ctx:&egui::Context){
        let texture=self.texture.as_ref().unwrap().clone();
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ctx,|ui|{
            let rect=ui.max_rect();
            let ratio=self.source[0] as f32/self.source[1] as f32;
            let size=if rect.width()/rect.height()>ratio{Vec2::new(rect.height()*ratio,rect.height())}else{Vec2::new(rect.width(),rect.width()/ratio)};
            let pixels=ctx.pixels_per_point();
            self.target_w.store((size.x*pixels).ceil() as u32,Ordering::Relaxed);
            self.target_h.store((size.y*pixels).ceil() as u32,Ordering::Relaxed);
            let dest=egui::Rect::from_center_size(rect.center(),size);
            ui.painter().image(texture.id(),dest,egui::Rect::from_min_max(egui::pos2(0.,0.),egui::pos2(1.,1.)),Color32::WHITE);
            let response=ui.interact(rect,ui.id().with("screen"),egui::Sense::click_and_drag());
            if response.drag_started() && !self.fullscreen {ctx.send_viewport_cmd(ViewportCommand::StartDrag);}
            if response.double_clicked(){self.fullscreen(ctx);}
            response.context_menu(|ui|{
                if ui.button("Fullscreen   F11").clicked(){self.fullscreen(ctx);ui.close();}
                if ui.checkbox(&mut self.pinned,"Always on top").changed(){ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.pinned{egui::WindowLevel::AlwaysOnTop}else{egui::WindowLevel::Normal}));}
                ui.separator();
                if ui.button("Disconnect").clicked(){self.disconnect();ui.close();}
                if ui.button("Close").clicked(){ctx.send_viewport_cmd(ViewportCommand::Close);}
            });
        });
        if ctx.input(|i|i.pointer.delta().length_sq()>0.){self.hover_until=Instant::now()+Duration::from_secs(2);}
        if Instant::now()<self.hover_until || self.stopping {
            egui::Area::new("screen-controls".into()).anchor(egui::Align2::CENTER_TOP,[0.,10.]).order(egui::Order::Foreground).show(ctx,|ui|{
                egui::Frame::new().fill(Color32::from_black_alpha(220)).corner_radius(10).inner_margin(8).show(ui,|ui|{
                    ui.horizontal(|ui|{
                        ui.label(RichText::new(if self.stopping{"Stopping…"}else{"● USB"}).color(MINT).size(11.));
                        if ui.small_button("⛶").on_hover_text("Fullscreen · F11").clicked(){self.fullscreen(ctx);}
                        if ui.small_button("Disconnect").clicked(){self.disconnect();}
                        if ui.small_button("×").clicked(){ctx.send_viewport_cmd(ViewportCommand::Close);}
                    });
                });
            });
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // Border resize remains available without permanent window chrome.
        if !self.fullscreen {
            let rect=ctx.content_rect();
            egui::Area::new("resize".into()).fixed_pos(rect.right_bottom()-Vec2::splat(14.)).order(egui::Order::Foreground).show(ctx,|ui|{
                let r=ui.allocate_response(Vec2::splat(14.),egui::Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeNwSe);
                if r.drag_started(){ctx.send_viewport_cmd(ViewportCommand::BeginResize(egui::ResizeDirection::SouthEast));}
            });
        }
    }
}

impl eframe::App for App {
    fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame){
        self.update_video(ctx);
        if ctx.input(|i|i.viewport().close_requested()) && self.worker.is_some(){
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);self.close_pending=true;self.disconnect();
        }
        if ctx.input(|i|i.key_pressed(egui::Key::F11)) && self.texture.is_some(){self.fullscreen(ctx);}
        if ctx.input(|i|i.key_pressed(egui::Key::Escape)) && self.fullscreen{self.fullscreen(ctx);}
        if self.texture.is_some(){self.floating_screen(ctx);}else{self.connection_card(ctx);}
        if self.smoke {
            let seconds=self.started.elapsed().as_secs_f32();
            match self.smoke_phase {
                0 => {self.connect(ctx);self.smoke_phase=1;},
                1 if seconds>10. => {self.fullscreen(ctx);self.smoke_phase=2;},
                2 if seconds>12. => {self.fullscreen(ctx);self.smoke_phase=3;},
                3 if seconds>17. => {
                    let s=self.shared.lock().unwrap();self.smoke_frames=s.frames;
                    crate::log(&format!("cycle1 frames={} first={:?} hardware={} source={:?} rendered={}",s.frames,s.first_frame,self.hardware,self.source,self.texture.is_some()));
                    drop(s);self.disconnect();self.smoke_phase=4;
                },
                4 if self.worker.is_none()=>{self.connect(ctx);self.smoke_phase=5;},
                5 if seconds>32. =>{
                    let s=self.shared.lock().unwrap();
                    crate::log(&format!("cycle2 frames={} first={:?} hardware={} rendered={}",s.frames,s.first_frame,self.hardware,self.texture.is_some()));
                    drop(s);self.close_pending=true;self.disconnect();self.smoke_phase=6;
                },
                _=>{},
            }
            if seconds>50. {self.disconnect();self.close_pending=true;}
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
