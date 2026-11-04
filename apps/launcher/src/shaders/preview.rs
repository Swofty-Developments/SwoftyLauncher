use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::shaders::CompiledEffect;
use swofty_preview::{ActiveInstance, Clip, ClipInfo, Frame, Preview};
use swofty_shaders::rules::{Engine, Signals};
use swofty_shaders::{Catalog, Rule, Target};

use super::clips::{self, ClipItem};
use super::triggers::Triggers;
use super::{Mode, ShadersView};
use crate::theme::Palette;

const POLL: Duration = Duration::from_millis(30);

#[derive(Clone, PartialEq)]
enum Status {
    Idle,
    Downloading,
    DownloadFailed(String),
    Opening,
    Ready,
    NoGpu(String),
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum Effects {
    NotStarted,
    Compiling,
    Ready,
    Failed(String),
}

enum Job {
    Open(PathBuf),
    Effects(Vec<CompiledEffect>, usize),
    Rules(Vec<Rule>),
    Triggers(Signals),
    Fire(String, f64),
    Instances(Vec<ActiveInstance>),
    Render {
        frame: usize,
        compare: bool,
        settle: bool,
    },
}

enum Done {
    Opened(Result<ClipInfo, String>),
    Ack,
    Skipped,
    Frame {
        index: usize,
        config: Arc<RenderImage>,
        original: Option<Arc<RenderImage>>,
        weights: Vec<f64>,
        busy: bool,
    },
    Failed {
        message: String,
        gpu: bool,
    },
}

struct Worker {
    jobs: Sender<Job>,
    done: Receiver<Done>,
}

pub(super) struct PreviewState {
    worker: Option<Worker>,
    clips: Vec<ClipItem>,
    scanned: bool,
    clip: usize,
    info: Option<ClipInfo>,
    frame: usize,
    pub(super) playing: bool,
    config_image: Option<Arc<RenderImage>>,
    original_image: Option<Arc<RenderImage>>,
    compare: bool,
    split: f32,
    dragging: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    status: Status,
    effects: Effects,
    instances: Vec<ActiveInstance>,
    pub(super) triggers: Triggers,
    slider: Entity<SliderState>,
    slider_events: Option<Subscription>,
    pending: usize,
    tick: Option<Task<()>>,
    poll: Option<Task<()>>,
    download: Option<Task<()>>,
    progress: Arc<Mutex<(f32, String)>>,
    render_error: Option<String>,
}

impl PreviewState {
    pub fn new(_window: &mut Window, cx: &mut Context<ShadersView>) -> Self {
        Self {
            worker: None,
            clips: Vec::new(),
            scanned: false,
            clip: 0,
            info: None,
            frame: 0,
            playing: true,
            config_image: None,
            original_image: None,
            compare: false,
            split: 0.5,
            dragging: false,
            bounds: Rc::new(Cell::new(Bounds::default())),
            status: Status::Idle,
            effects: Effects::NotStarted,
            instances: Vec::new(),
            triggers: Triggers::default(),
            slider: cx.new(|_| SliderState::new().min(0.).max(1.).step(1.)),
            slider_events: None,
            pending: 0,
            tick: None,
            poll: None,
            download: None,
            progress: Arc::new(Mutex::new((0., String::new()))),
            render_error: None,
        }
    }
}

/// A frame as an image the window draws as it is: gpui takes BGRA.
fn image_of(frame: &Frame) -> Result<Arc<RenderImage>, String> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if frame.rgba.len() < w * h * 4 || w == 0 || h == 0 {
        return Err("The frame is empty".into());
    }
    let mut bgra = frame.rgba[..w * h * 4].to_vec();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, bgra).ok_or("The frame is empty")?;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

const STEP: usize = 8;

struct Want {
    index: usize,
    compare: bool,
    settle: bool,
}

#[derive(Default)]
struct Rig {
    rules: Vec<Rule>,
    engine: Option<Engine>,
    base: Vec<ActiveInstance>,
    signals: Signals,
    pending: Vec<(String, f64)>,
    last: Option<usize>,
    applied: Option<Vec<ActiveInstance>>,
    settle_left: f64,
    last_settle: Option<Instant>,
}

impl Rig {
    fn set_rules(&mut self, rules: Vec<Rule>) {
        self.engine = (!rules.is_empty()).then(|| Engine::new(&rules));
        self.rules = rules;
        self.pending.clear();
        self.last = None;
        self.applied = None;
    }

    fn touched(&mut self) {
        let hold = self
            .rules
            .iter()
            .map(|r| r.hold_ms + r.min_ms + r.for_ms)
            .fold(0.0, f64::max);
        self.settle_left = hold + 150.0;
    }

    fn drive(&mut self, p: &mut Preview, index: usize, fps: f32, settle: bool) -> (Vec<f64>, bool) {
        let Some(engine) = self.engine.as_mut() else {
            return (Vec::new(), false);
        };
        let dt = match self.last {
            Some(l) if index > l && index - l <= STEP => {
                Some((index - l) as f64 * 1000.0 / f64::from(fps.max(1.0)))
            }
            Some(l) if index == l && settle => Some(
                self.last_settle
                    .map_or(33.0, |t| t.elapsed().as_secs_f64() * 1000.0)
                    .clamp(1.0, 100.0),
            ),
            Some(l) if index == l => None,
            _ => {
                engine.reset();
                Some(0.0)
            }
        };
        if let Some(dt) = dt {
            let mut signals = self.signals.clone();
            for (id, magnitude) in self.pending.drain(..) {
                signals.fire(&id, magnitude);
            }
            engine.step(dt, &signals);
            self.settle_left = (self.settle_left - dt).max(0.0);
        }
        self.last_settle = Some(Instant::now());
        self.last = Some(index);
        let (blended, off) = engine.apply(&self.base);
        let shown: Vec<ActiveInstance> = blended
            .into_iter()
            .zip(off)
            .filter_map(|(i, off)| (!off).then_some(i))
            .collect();
        if self.applied.as_ref() != Some(&shown) {
            p.set_instances(shown.clone());
            self.applied = Some(shown);
        }
        let weights = engine.weights().to_vec();
        let busy = self.settle_left > 0.0
            || weights.iter().any(|&w| w > 0.0 && w < 1.0)
            || self
                .rules
                .iter()
                .enumerate()
                .any(|(i, r)| r.event && engine.is_on(i));
        (weights, busy)
    }
}

fn work(jobs: Receiver<Job>, done: Sender<Done>) {
    let mut clip: Option<Clip> = None;
    let mut preview: Option<Preview> = None;
    let mut gpu_error: Option<String> = None;
    let mut original: Option<(usize, Arc<RenderImage>)> = None;
    let mut logged_failure: Option<String> = None;
    let mut rig = Rig::default();
    while let Ok(first) = jobs.recv() {
        let mut batch = vec![first];
        while let Ok(job) = jobs.try_recv() {
            batch.push(job);
        }
        let last_render = batch.iter().rposition(|j| matches!(j, Job::Render { .. }));
        for (i, job) in batch.into_iter().enumerate() {
            let reply = match job {
                Job::Render { .. } if Some(i) != last_render => Done::Skipped,
                Job::Open(dir) => {
                    original = None;
                    rig.last = None;
                    match Clip::open(&dir) {
                        Ok(opened) => {
                            let info = opened.info.clone();
                            clip = Some(opened);
                            match gpu(&mut preview, &mut gpu_error) {
                                Ok(p) => {
                                    p.set_clip(&info);
                                    Done::Opened(Ok(info))
                                }
                                Err(message) => Done::Failed { message, gpu: true },
                            }
                        }
                        Err(e) => {
                            clip = None;
                            Done::Opened(Err(e))
                        }
                    }
                }
                Job::Effects(effects, rules) => match gpu(&mut preview, &mut gpu_error) {
                    Ok(p) => {
                        crate::app_log::note(&describe(&effects, rules));
                        p.set_effects(effects);
                        Done::Ack
                    }
                    Err(message) => Done::Failed { message, gpu: true },
                },
                Job::Rules(rules) => {
                    rig.set_rules(rules);
                    Done::Ack
                }
                Job::Triggers(signals) => {
                    rig.signals = signals;
                    rig.touched();
                    Done::Ack
                }
                Job::Fire(id, magnitude) => {
                    rig.pending.push((id, magnitude));
                    rig.touched();
                    Done::Ack
                }
                Job::Instances(instances) => match gpu(&mut preview, &mut gpu_error) {
                    Ok(p) => {
                        rig.applied = None;
                        if rig.engine.is_none() {
                            p.set_instances(instances.clone());
                        }
                        rig.base = instances;
                        Done::Ack
                    }
                    Err(message) => Done::Failed { message, gpu: true },
                },
                Job::Render {
                    frame,
                    compare,
                    settle,
                } => render(
                    &mut clip,
                    &mut preview,
                    &mut gpu_error,
                    &mut original,
                    &mut rig,
                    Want {
                        index: frame,
                        compare,
                        settle,
                    },
                ),
            };
            if let Done::Failed { message, .. } = &reply
                && logged_failure.as_ref() != Some(message)
            {
                crate::app_log::note(&format!("[shaders] preview: {message}"));
                logged_failure = Some(message.clone());
            }
            if done.send(reply).is_err() {
                return;
            }
        }
    }
}

/// The effects the preview loads, each with its textures: a declared one's format, or an image's file and size.
fn describe(effects: &[CompiledEffect], rules: usize) -> String {
    let list: Vec<String> = effects
        .iter()
        .map(|e| {
            let textures: Vec<String> = e
                .effect
                .textures
                .iter()
                .map(|t| match &t.file {
                    Some(file) => match e.images.iter().find(|i| i.0 == t.name) {
                        Some((_, w, h, _)) => format!("{} {file} {w}x{h}", t.name),
                        None => format!("{} {file} not loaded", t.name),
                    },
                    None => format!("{} {}", t.name, t.format.as_deref().unwrap_or("rgba16f")),
                })
                .collect();
            if textures.is_empty() {
                e.effect.id.clone()
            } else {
                format!("{} ({})", e.effect.id, textures.join(", "))
            }
        })
        .collect();
    format!("[shaders] preview effects: {}; {rules} rule(s)", list.join("; "))
}

fn gpu<'a>(preview: &'a mut Option<Preview>, error: &mut Option<String>) -> Result<&'a mut Preview, String> {
    if let Some(e) = error {
        return Err(e.clone());
    }
    if preview.is_none() {
        match Preview::new() {
            Ok(p) => {
                crate::app_log::note(&format!("[shaders] preview runs on {}", p.adapter_name()));
                *preview = Some(p);
            }
            Err(e) => {
                crate::app_log::note(&format!("[shaders] preview has no GPU: {e}"));
                *error = Some(e.clone());
                return Err(e);
            }
        }
    }
    preview.as_mut().ok_or_else(|| "No GPU".to_owned())
}

fn render(
    clip: &mut Option<Clip>,
    preview: &mut Option<Preview>,
    gpu_error: &mut Option<String>,
    original: &mut Option<(usize, Arc<RenderImage>)>,
    rig: &mut Rig,
    want: Want,
) -> Done {
    let Want {
        index,
        compare,
        settle,
    } = want;
    let Some(clip) = clip.as_ref() else {
        return Done::Failed {
            message: "No clip is open".into(),
            gpu: false,
        };
    };
    let p = match gpu(preview, gpu_error) {
        Ok(p) => p,
        Err(message) => return Done::Failed { message, gpu: true },
    };
    let index = index.min(clip.len().saturating_sub(1));
    let frame = match clip.frame(index) {
        Ok(f) => f,
        Err(message) => return Done::Failed { message, gpu: false },
    };
    let (weights, busy) = rig.drive(p, index, clip.info.fps, settle);
    let out = match p.render(&frame).and_then(|f| image_of(&f)) {
        Ok(b) => b,
        Err(message) => return Done::Failed { message, gpu: false },
    };
    let original_png = if compare {
        match original {
            Some((i, image)) if *i == index => Some(image.clone()),
            _ => match image_of(&frame) {
                Ok(image) => {
                    *original = Some((index, image.clone()));
                    Some(image)
                }
                Err(message) => return Done::Failed { message, gpu: false },
            },
        }
    } else {
        None
    };
    Done::Frame {
        index,
        config: out,
        original: original_png,
        weights,
        busy,
    }
}

/// Library and addon effects compiled to SPIR-V for the preview.
fn compile_effects(
    store: &swofty_core::store::Store,
    _catalog: &Catalog,
) -> Result<Vec<CompiledEffect>, String> {
    let (effects, warnings) = swofty_core::shaders::compile_all(store, Target::Spirv);
    if effects.is_empty() {
        return Err(warnings
            .into_iter()
            .next()
            .unwrap_or_else(|| "There are no effects to preview".into()));
    }
    Ok(effects)
}

/// Shows `next` in `slot` and frees the image it replaces from the window's texture atlas.
fn show(slot: &mut Option<Arc<RenderImage>>, next: Option<Arc<RenderImage>>, cx: &mut App) {
    if let Some(old) = std::mem::replace(slot, next)
        && slot.as_ref().is_none_or(|n| !Arc::ptr_eq(n, &old))
    {
        cx.drop_image(old, None);
    }
}

fn clock(frame: usize, fps: f32) -> String {
    let secs = frame as f32 / fps.max(1.);
    format!("{}:{:04.1}", (secs / 60.) as u32, secs % 60.)
}

impl ShadersView {
    fn worker(&mut self) -> &Worker {
        self.preview.worker.get_or_insert_with(|| {
            let (jobs, job_rx) = mpsc::channel();
            let (done_tx, done) = mpsc::channel();
            std::thread::Builder::new()
                .name("shaders-preview".into())
                .spawn(move || work(job_rx, done_tx))
                .ok();
            Worker { jobs, done }
        })
    }

    fn send(&mut self, job: Job, cx: &mut Context<Self>) {
        if self.worker().jobs.send(job).is_err() {
            self.preview.worker = None;
            self.preview.status =
                Status::Failed("The preview stopped. Reopen the page to restart it.".into());
            return;
        }
        self.preview.pending += 1;
        if self.preview.poll.is_none() {
            self.preview.poll = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(POLL).await;
                    let more = this.update(cx, |v, cx| v.drain(cx)).unwrap_or(false);
                    if !more {
                        break;
                    }
                }
                this.update(cx, |v, _| v.preview.poll = None).ok();
            }));
        }
    }

    fn drain(&mut self, cx: &mut Context<Self>) -> bool {
        let mut replies = Vec::new();
        if let Some(w) = &self.preview.worker {
            while let Ok(d) = w.done.try_recv() {
                replies.push(d);
            }
        }
        if replies.is_empty() {
            return self.preview.pending > 0;
        }
        for reply in replies {
            self.preview.pending = self.preview.pending.saturating_sub(1);
            self.reply(reply, cx);
        }
        cx.notify();
        self.preview.pending > 0
    }

    fn reply(&mut self, reply: Done, cx: &mut Context<Self>) {
        match reply {
            Done::Ack | Done::Skipped => {}
            Done::Opened(Ok(info)) => {
                self.preview.status = Status::Ready;
                self.preview.frame = 0;
                let last = info.frames.saturating_sub(1) as f32;
                self.preview.slider = cx.new(|_| SliderState::new().min(0.).max(last.max(1.)).step(1.));
                self.preview.slider_events = Some(cx.subscribe(
                    &self.preview.slider,
                    |this, _, ev: &SliderEvent, cx| {
                        let (SliderEvent::Change(v) | SliderEvent::Release(v)) = ev;
                        this.preview_seek(v.start().round().max(0.) as usize, cx);
                    },
                ));
                self.preview.info = Some(info);
                self.preview_send_instances(cx);
                self.preview_render(cx);
                if self.preview.playing && self.mode == Mode::Preview {
                    self.preview_play(cx);
                }
            }
            Done::Opened(Err(e)) => {
                self.preview.status = Status::Failed(format!("Couldn't open the clip: {e}"))
            }
            Done::Frame {
                index,
                config,
                original,
                weights,
                busy,
            } => {
                show(&mut self.preview.config_image, Some(config), cx);
                if let Some(o) = original {
                    show(&mut self.preview.original_image, Some(o), cx);
                }
                self.preview.render_error = None;
                let _ = index;
                self.preview.triggers.weights = weights;
                self.preview.triggers.busy = busy;
                if !self.preview.playing {
                    self.fade_tick(cx);
                }
            }
            Done::Failed { message, gpu: true } => self.preview.status = Status::NoGpu(message),
            Done::Failed { message, gpu: false } => self.preview.render_error = Some(message),
        }
    }

    pub(super) fn preview_open(&mut self, cx: &mut Context<Self>) {
        if !self.preview.scanned {
            self.preview.scanned = true;
            let root = self.store.root.clone();
            cx.spawn(async move |this, cx| {
                let found = cx
                    .background_executor()
                    .spawn(async move { clips::scan(&root) })
                    .await;
                this.update(cx, |v, cx| v.scanned(found, cx)).ok();
            })
            .detach();
        } else if self.preview.info.is_some() && self.preview.status == Status::Ready {
            self.preview_play(cx);
        }
        self.preview_compile(cx);
    }

    pub(super) fn preview_pause(&mut self, _cx: &mut Context<Self>) {
        self.preview.tick = None;
    }

    fn scanned(&mut self, found: Vec<ClipItem>, cx: &mut Context<Self>) {
        self.preview.clips = found;
        if self.preview.clips.is_empty() {
            self.preview_download(cx);
        } else {
            self.preview_pick(0, cx);
            self.preview_fetch_rest(cx);
        }
        cx.notify();
    }

    fn preview_pick(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(item) = self.preview.clips.get(i).cloned() else {
            return;
        };
        self.preview.clip = i;
        self.preview.status = Status::Opening;
        show(&mut self.preview.config_image, None, cx);
        show(&mut self.preview.original_image, None, cx);
        self.preview.tick = None;
        self.send(Job::Open(item.dir), cx);
        cx.notify();
    }

    fn preview_download(&mut self, cx: &mut Context<Self>) {
        if self.preview.status == Status::Downloading {
            return;
        }
        self.preview.status = Status::Downloading;
        let root = self.store.root.clone();
        let progress = self.preview.progress.clone();
        if let Ok(mut p) = progress.lock() {
            *p = (0., "Looking for clips".into());
        }
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                let going = this
                    .update(cx, |v, cx| {
                        cx.notify();
                        v.preview.status == Status::Downloading
                    })
                    .unwrap_or(false);
                if !going {
                    break;
                }
            }
        });
        self.preview.download = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    clips::download(&root, true, &|f, label| {
                        if let Ok(mut p) = progress.lock() {
                            *p = (f, label.to_owned());
                        }
                    })?;
                    Ok::<Vec<ClipItem>, String>(clips::scan(&root))
                })
                .await;
            drop(ticker);
            let rest = result.is_ok();
            this.update(cx, |v, cx| {
                match result {
                    Ok(found) if !found.is_empty() => {
                        v.preview.clips = found;
                        v.preview_pick(0, cx);
                    }
                    Ok(_) => v.preview.status = Status::DownloadFailed("The clip index is empty.".into()),
                    Err(e) => v.preview.status = Status::DownloadFailed(e),
                }
                cx.notify();
            })
            .ok();
            if rest {
                this.update(cx, |v, cx| v.preview_fetch_rest(cx)).ok();
            }
        }));
    }

    /// Fetches the clips of the CDN index that aren't cached, quietly while one plays; they join the
    /// clip picker.
    fn preview_fetch_rest(&mut self, cx: &mut Context<Self>) {
        let root = self.store.root.clone();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    clips::download(&root, false, &|_, _| {}).ok();
                    clips::scan(&root)
                })
                .await;
            this.update(cx, |v, cx| {
                let ids = |c: &[ClipItem]| c.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
                if ids(&found) != ids(&v.preview.clips) {
                    let playing = v.preview.clips.get(v.preview.clip).map(|c| c.id.clone());
                    v.preview.clips = found;
                    if let Some(i) = playing.and_then(|id| v.preview.clips.iter().position(|c| c.id == id)) {
                        v.preview.clip = i;
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn preview_catalog_changed(&mut self, cx: &mut Context<Self>) {
        self.preview.effects = Effects::NotStarted;
        if self.preview.scanned {
            self.preview_compile(cx);
        }
    }

    fn preview_compile(&mut self, cx: &mut Context<Self>) {
        if self.preview.effects != Effects::NotStarted || self.catalog.effects.is_empty() {
            return;
        }
        self.preview.effects = Effects::Compiling;
        let store = self.store.clone();
        let catalog = self.catalog.clone();
        cx.spawn(async move |this, cx| {
            let compiled = cx
                .background_executor()
                .spawn(async move { compile_effects(&store, &catalog) })
                .await;
            this.update(cx, |v, cx| {
                match compiled {
                    Ok(effects) => {
                        v.preview.effects = Effects::Ready;
                        let rules = v.preview.triggers.rules.len();
                        v.send(Job::Effects(effects, rules), cx);
                        v.preview_send_instances(cx);
                        v.preview_render(cx);
                    }
                    Err(e) => v.preview.effects = Effects::Failed(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn preview_instances(
        &mut self,
        instances: Vec<ActiveInstance>,
        rules: Vec<Rule>,
        cx: &mut Context<Self>,
    ) {
        let rules_changed = self.trigger_rules(rules, cx);
        if self.preview.instances == instances && !rules_changed {
            return;
        }
        self.preview.instances = instances;
        if self.preview.worker.is_some() {
            self.preview_send_instances(cx);
            self.preview_render(cx);
        }
    }

    fn preview_send_instances(&mut self, cx: &mut Context<Self>) {
        let rules = self.preview.triggers.rules.clone();
        self.send(Job::Rules(rules), cx);
        self.preview_send_triggers(cx);
        let instances = self.preview.instances.clone();
        self.send(Job::Instances(instances), cx);
    }

    pub(super) fn preview_send_triggers(&mut self, cx: &mut Context<Self>) {
        if self.preview.worker.is_none() {
            return;
        }
        let signals = self.trigger_signals();
        self.send(Job::Triggers(signals), cx);
    }

    pub(super) fn preview_fire(&mut self, id: &str, magnitude: f64, cx: &mut Context<Self>) {
        if self.preview.worker.is_some() {
            self.send(Job::Fire(id.to_owned(), magnitude), cx);
        }
    }

    fn preview_render(&mut self, cx: &mut Context<Self>) {
        if self.preview.info.is_none() || self.preview.status != Status::Ready {
            return;
        }
        let (frame, compare) = (self.preview.frame, self.preview.compare);
        self.send(
            Job::Render {
                frame,
                compare,
                settle: false,
            },
            cx,
        );
    }

    pub(super) fn preview_render_settle(&mut self, cx: &mut Context<Self>) {
        if self.preview.info.is_none() || self.preview.status != Status::Ready {
            return;
        }
        let (frame, compare) = (self.preview.frame, self.preview.compare);
        self.send(
            Job::Render {
                frame,
                compare,
                settle: true,
            },
            cx,
        );
    }

    fn preview_seek(&mut self, frame: usize, cx: &mut Context<Self>) {
        if self.preview.frame == frame {
            return;
        }
        self.preview.frame = frame;
        self.preview_render(cx);
        cx.notify();
    }

    fn preview_play(&mut self, cx: &mut Context<Self>) {
        let Some(info) = self.preview.info.clone() else {
            return;
        };
        let frames = info.frames.max(1);
        let fps = info.fps.max(1.);
        let (from, started) = (self.preview.frame, Instant::now());
        self.preview.tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs_f32((1. / fps).clamp(0.02, 0.1)))
                    .await;
                let at = (from + (started.elapsed().as_secs_f32() * fps) as usize) % frames;
                let alive = this
                    .update(cx, |v, cx| {
                        if !v.preview.playing || v.preview.dragging {
                            return v.preview.playing;
                        }
                        if v.preview.frame != at {
                            v.preview.frame = at;
                            v.preview_render(cx);
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        }));
    }

    pub(super) fn sync_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at = self.preview.frame as f32;
        let slider = self.preview.slider.clone();
        if (slider.read(cx).value().start() - at).abs() > 0.5 {
            slider.update(cx, |s, cx| s.set_value(at, window, cx));
        }
    }

    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        self.preview.playing = !self.preview.playing;
        if self.preview.playing {
            self.preview_play(cx);
        } else {
            self.preview.tick = None;
            self.fade_tick(cx);
        }
        cx.notify();
    }

    fn toggle_compare(&mut self, cx: &mut Context<Self>) {
        self.preview.compare = !self.preview.compare;
        self.preview_render(cx);
        cx.notify();
    }

    fn wipe_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let b = self.preview.bounds.get();
        let width = f32::from(b.size.width);
        if width > 1. {
            self.preview.split = ((f32::from(x) - f32::from(b.origin.x)) / width).clamp(0.02, 0.98);
            cx.notify();
        }
    }

    pub(super) fn preview_pane(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let status = self.preview.status.clone();
        let body = match &status {
            Status::Idle | Status::Opening => {
                self.preview_message(Icon::new(IconName::LoaderCircle), "Opening the clip", None, p)
            }
            Status::Downloading => self.download_card(p),
            Status::DownloadFailed(e) => {
                let retry = Button::new("shaders-clips-retry")
                    .outline()
                    .icon(IconName::RotateCw)
                    .label("Try again")
                    .on_click(cx.listener(|this, _, _, cx| this.preview_download(cx)));
                self.preview_message(
                    Icon::new(IconName::CloudOff),
                    "The preview clips couldn't be downloaded",
                    Some((e.clone(), retry.into_any_element())),
                    p,
                )
            }
            Status::NoGpu(e) => self.preview_message(
                Icon::new(IconName::Cpu),
                "The preview can't use the graphics card",
                Some((e.clone(), div().into_any_element())),
                p,
            ),
            Status::Failed(e) => self.preview_message(
                Icon::new(IconName::CircleAlert),
                "The preview can't run",
                Some((e.clone(), div().into_any_element())),
                p,
            ),
            Status::Ready => self.viewport(p, cx),
        };
        let notes = self.preview_notes(p);
        let strip = self.triggers_strip(p, cx);
        v_flex()
            .size_full()
            .min_h_0()
            .gap(px(10.))
            .child(body)
            .children(notes)
            .child(self.transport(p, cx))
            .children(strip)
            .into_any_element()
    }

    fn preview_notes(&self, p: Palette) -> Option<AnyElement> {
        let line = match (&self.preview.effects, &self.preview.render_error) {
            (Effects::Failed(e), _) => Some((IconName::TriangleAlert, p.warning, e.clone())),
            (_, Some(e)) => Some((IconName::TriangleAlert, p.warning, e.clone())),
            _ if self.editor.lint.errors() > 0 => Some((
                IconName::Info,
                p.text_muted,
                "The preview shows the last config without problems.".to_owned(),
            )),
            _ => None,
        }?;
        Some(
            h_flex()
                .gap(px(8.))
                .items_center()
                .text_size(px(12.))
                .text_color(line.1)
                .child(Icon::new(line.0).size(px(13.)))
                .child(div().flex_1().min_w_0().child(line.2))
                .into_any_element(),
        )
    }

    fn preview_message(
        &self,
        icon: Icon,
        title: &'static str,
        detail: Option<(String, AnyElement)>,
        p: Palette,
    ) -> AnyElement {
        v_flex()
            .w_full()
            .flex_1()
            .min_h(px(240.))
            .items_center()
            .justify_center()
            .gap(px(6.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(icon.size(px(24.)).text_color(p.text_placeholder))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .when_some(detail, |d, (text, action)| {
                d.child(
                    div()
                        .max_w(px(420.))
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .text_center()
                        .child(text),
                )
                .child(div().pt(px(6.)).child(action))
            })
            .into_any_element()
    }

    fn download_card(&self, p: Palette) -> AnyElement {
        let (fraction, label) = self
            .preview
            .progress
            .lock()
            .map(|g| g.clone())
            .unwrap_or((0., String::new()));
        v_flex()
            .w_full()
            .flex_1()
            .min_h(px(240.))
            .items_center()
            .justify_center()
            .gap(px(8.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(Icon::new(IconName::Download).size(px(24.)).text_color(p.accent))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Getting the preview clips"),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child("A few seconds of F1 25 footage to try your shaders on. This happens once."),
            )
            .child(
                div()
                    .mt(px(8.))
                    .w(px(280.))
                    .h(px(6.))
                    .rounded(px(3.))
                    .bg(p.elevated)
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(relative(fraction.clamp(0.02, 1.)))
                            .rounded(px(3.))
                            .bg(p.accent),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(label),
            )
            .into_any_element()
    }

    fn viewport(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let bounds = self.preview.bounds.clone();
        let compare = self.preview.compare && self.preview.original_image.is_some();
        let split = self.preview.split.clamp(0.02, 0.98);
        let config = self.preview.config_image.clone();
        let original = self.preview.original_image.clone();
        let ratio = self
            .preview
            .info
            .as_ref()
            .filter(|i| i.height > 0)
            .map_or(16. / 9., |i| i.width as f32 / i.height as f32);
        let frame = div()
            .id("shaders-viewport")
            .relative()
            .w_full()
            .aspect_ratio(ratio)
            .rounded(px(10.))
            .overflow_hidden()
            .bg(p.chrome)
            .border_1()
            .border_color(p.border)
            .child(match config {
                Some(image) => img(image)
                    .size_full()
                    .object_fit(ObjectFit::Fill)
                    .into_any_element(),
                None => div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::LoaderCircle)
                            .size(px(20.))
                            .text_color(p.text_placeholder),
                    )
                    .into_any_element(),
            })
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            );
        let frame = if compare {
            let drag_start = cx.listener(|this, e: &MouseDownEvent, _, cx| {
                this.preview.dragging = true;
                this.wipe_to(e.position.x, cx);
            });
            frame
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .h_full()
                        .w(relative(split))
                        .overflow_hidden()
                        .when_some(original, |d, image| {
                            d.child(
                                img(image)
                                    .h_full()
                                    .w(relative(1. / split))
                                    .flex_shrink_0()
                                    .object_fit(ObjectFit::Fill),
                            )
                        }),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(relative(split))
                        .w(px(2.))
                        .ml(px(-1.))
                        .bg(p.on_accent),
                )
                .child(
                    div()
                        .absolute()
                        .top(relative(0.5))
                        .left(relative(split))
                        .size(px(28.))
                        .ml(px(-14.))
                        .mt(px(-14.))
                        .rounded_full()
                        .bg(p.accent)
                        .border_2()
                        .border_color(p.on_accent)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::ChevronsUpDown)
                                .size(px(14.))
                                .text_color(p.on_accent),
                        ),
                )
                .child(wipe_label("Original", true, p))
                .child(wipe_label("Config", false, p))
                .cursor_col_resize()
                .on_mouse_down(MouseButton::Left, drag_start)
                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                    if this.preview.dragging && e.dragging() {
                        this.wipe_to(e.position.x, cx);
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.preview.dragging = false;
                        cx.notify();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.preview.dragging = false;
                        cx.notify();
                    }),
                )
        } else {
            frame
        };
        div().w_full().child(frame).into_any_element()
    }

    fn transport(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let ready = self.preview.status == Status::Ready && self.preview.info.is_some();
        let (frames, fps) = self.preview.info.as_ref().map_or((0, 25.), |i| (i.frames, i.fps));
        let time = format!(
            "{} / {}",
            clock(self.preview.frame, fps),
            clock(frames.saturating_sub(1), fps)
        );
        let playing = self.preview.playing;
        let compare = self.preview.compare;
        let titles: Vec<String> = self.preview.clips.iter().map(|c| c.title.clone()).collect();
        let current = titles
            .get(self.preview.clip)
            .cloned()
            .unwrap_or_else(|| "Clip".into());
        let shaders = cx.entity().downgrade();
        let at = self.preview.clip;
        let picker = Button::new("shaders-clip")
            .outline()
            .small()
            .icon(IconName::Flag)
            .label(current)
            .dropdown_caret(true)
            .disabled(titles.len() < 2)
            .dropdown_menu_with_anchor(Anchor::BottomRight, move |mut menu, _, _| {
                for (i, title) in titles.iter().enumerate() {
                    let shaders = shaders.clone();
                    menu = menu.item(PopupMenuItem::new(title.clone()).checked(i == at).on_click(
                        move |_, _, cx| {
                            shaders.update(cx, |v, cx| v.preview_pick(i, cx)).ok();
                        },
                    ));
                }
                menu
            });
        h_flex()
            .flex_shrink_0()
            .gap(px(10.))
            .items_center()
            .child(
                Button::new("shaders-play")
                    .outline()
                    .small()
                    .icon(if playing { IconName::Pause } else { IconName::Play })
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_play(cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Slider::new(&self.preview.slider).disabled(!ready)),
            )
            .child(
                div()
                    .w(px(112.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(time),
            )
            .child(
                Button::new("shaders-compare")
                    .small()
                    .map(|b| if compare { b } else { b.outline() })
                    .icon(IconName::ArrowRightLeft)
                    .label("Compare")
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_compare(cx))),
            )
            .child(picker)
            .into_any_element()
    }
}

fn wipe_label(text: &'static str, left: bool, p: Palette) -> Div {
    div()
        .absolute()
        .top(px(10.))
        .map(|d| if left { d.left(px(10.)) } else { d.right(px(10.)) })
        .px(px(8.))
        .py(px(3.))
        .rounded(px(4.))
        .bg(p.chrome.opacity(0.75))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text)
        .child(text)
}
