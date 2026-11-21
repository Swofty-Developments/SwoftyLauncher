mod effects;
mod kernels;
mod plan;
mod textures;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use swofty_shaders::ActiveInstance;

use self::effects::{EffectGpu, Inst, layout_for, sampler};
use self::kernels::KERNELS;
use self::plan::{Clock, dispatch};
use self::textures::{NARROW, Targets, Tex, clear};
use crate::{ClipInfo, CompiledEffect, Frame};

/// The most frames playback may skip between two renders and still count as continuing.
const STEP: usize = 8;

const UNIFORM_DWORDS: usize = 56;
const UNIFORM_STRIDE: u64 = 256;
const UNIFORM_BYTES: u64 = (UNIFORM_DWORDS * 4) as u64;
const INPUTS: usize = 8;

const PICTURE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BYTE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The renderer: effects and instances in, frames in, frames out.
pub struct Preview {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    errors: Arc<Mutex<Vec<String>>>,
    samplers: [wgpu::Sampler; 4],
    dummy: Tex,
    depth: Tex,
    layout: wgpu::BindGroupLayout,
    up: wgpu::ComputePipeline,
    down: wgpu::ComputePipeline,
    mip: wgpu::ComputePipeline,
    narrow: HashMap<&'static str, wgpu::ComputePipeline>,
    uniforms: wgpu::Buffer,
    uniform_slots: u64,
    sources: HashMap<String, CompiledEffect>,
    effects: HashMap<String, EffectGpu>,
    requested: Vec<ActiveInstance>,
    instances: Vec<Inst>,
    instances_dirty: bool,
    targets: Option<Targets>,
    last_frame: Option<usize>,
    fps: f32,
    view: u32,
}

fn scaled(base: u32, scale: f64) -> u32 {
    ((f64::from(base) * scale).round() as u32).max(1)
}

impl Preview {
    /// Opens a device. Blocks on the adapter and device requests; call it off the UI thread.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no graphics adapter for the preview: {e}"))?;
        let adapter_name = adapter.get_info().name;
        let needed = wgpu::Features::CLEAR_TEXTURE | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        if !adapter.features().contains(needed) {
            return Err(format!(
                "{adapter_name} cannot run the preview: it lacks {:?}",
                needed - adapter.features()
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("swofty preview"),
            required_features: needed,
            ..Default::default()
        }))
        .map_err(|e| format!("preview device: {e}"))?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        {
            let errors = errors.clone();
            device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                if let Ok(mut list) = errors.lock() {
                    list.push(e.to_string());
                }
            }));
        }
        let samplers = [
            sampler(&device, wgpu::FilterMode::Linear, wgpu::AddressMode::ClampToEdge),
            sampler(&device, wgpu::FilterMode::Nearest, wgpu::AddressMode::ClampToEdge),
            sampler(&device, wgpu::FilterMode::Linear, wgpu::AddressMode::Repeat),
            sampler(&device, wgpu::FilterMode::Nearest, wgpu::AddressMode::Repeat),
        ];
        let dummy = Tex::new(&device, "dummy", 1, 1, PICTURE);
        let depth = Tex::new(&device, "depth", 1, 1, wgpu::TextureFormat::R32Float);
        let layout = layout_for(&device);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("preview helpers"),
            source: wgpu::ShaderSource::Wgsl(KERNELS.into()),
        });
        let kernel = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let up = kernel("up");
        let down = kernel("down");
        let mip = kernel("mip");
        let narrow = NARROW.iter().map(|&entry| (entry, kernel(entry))).collect();
        let uniforms = Self::uniform_buffer(&device, 32);
        let preview = Self {
            device,
            queue,
            adapter_name,
            errors,
            samplers,
            dummy,
            depth,
            layout,
            up,
            down,
            mip,
            narrow,
            uniforms,
            uniform_slots: 32,
            sources: HashMap::new(),
            effects: HashMap::new(),
            requested: Vec::new(),
            instances: Vec::new(),
            instances_dirty: false,
            targets: None,
            last_frame: None,
            fps: 30.0,
            view: 0,
        };
        preview.take_errors()?;
        Ok(preview)
    }

    fn uniform_buffer(device: &wgpu::Device, slots: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("swofty cbuffer"),
            size: slots * UNIFORM_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn take_errors(&self) -> Result<(), String> {
        let mut list = self.errors.lock().map_err(|e| e.to_string())?;
        if list.is_empty() {
            return Ok(());
        }
        let message = list.join("; ");
        list.clear();
        Err(message)
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// The clip's frame rate and view feed `swofty_time`, `swofty_frametime` and `swofty_view`.
    pub fn set_clip(&mut self, info: &ClipInfo) {
        self.fps = if info.fps > 0.0 { info.fps } else { 30.0 };
        self.view = match info.view.as_str() {
            "cockpit" => 1,
            "tcam" => 2,
            "tcam_offset" => 3,
            "chase_near" => 4,
            "chase_far" => 5,
            "other" => 6,
            "trackside" => 7,
            "reverse_pod" => 8,
            "nose" => 9,
            "nose_alt" => 10,
            "wing" => 11,
            _ => 0,
        };
        self.last_frame = None;
    }

    /// Replaces the effect library. Pipelines are built the first time an instance uses an effect.
    pub fn set_effects(&mut self, effects: Vec<CompiledEffect>) {
        self.sources = effects.into_iter().map(|e| (e.effect.id.clone(), e)).collect();
        self.effects.clear();
        self.instances_dirty = true;
    }

    pub fn set_instances(&mut self, instances: Vec<ActiveInstance>) {
        let same = instances.len() == self.requested.len()
            && instances
                .iter()
                .zip(&self.requested)
                .all(|(a, b)| a.effect == b.effect && a.label == b.label && a.params.len() == b.params.len());
        if same && !self.instances_dirty {
            for (inst, new) in self.instances.iter_mut().zip(&instances) {
                inst.params.clone_from(&new.params);
            }
        } else {
            self.instances_dirty = true;
        }
        self.requested = instances;
    }

    /// The frame as it is, for the left side of the compare wipe.
    pub fn render_original(&self, frame: &Frame) -> Frame {
        frame.clone()
    }

    /// The frame with the instances applied. A frame up to `STEP` frames after the last one rendered
    /// continues it, as playback that drops frames does; any other frame (a scrub back or a long jump)
    /// resets `previous` and the persistent textures.
    pub fn render(&mut self, frame: &Frame) -> Result<Frame, String> {
        if frame.rgba.len() != (frame.width * frame.height * 4) as usize {
            return Err("the frame's pixel data does not match its size".into());
        }
        self.ensure_targets(frame.width, frame.height);
        if self.instances_dirty {
            self.build_instances(frame.width, frame.height)?;
        }
        let step = self
            .last_frame
            .filter(|&l| frame.index > l && frame.index - l <= STEP)
            .map(|l| frame.index - l);
        let consecutive = step.is_some();
        self.grow_uniforms();

        let targets = self.targets.as_ref().ok_or("no render targets")?;
        let (width, height) = (targets.width, targets.height);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preview"),
            });
        self.upload(targets, frame);
        if !consecutive {
            self.clear_history(&mut encoder, targets);
        }

        let up = self.bind_group(
            &self.up,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&targets.upload.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&targets.original.view),
                },
            ],
        );
        dispatch(&mut encoder, &self.up, &up, width, height, (8, 8));

        let clock = Clock {
            index: frame.index,
            time: frame.index as f32 / self.fps,
            frametime: step.unwrap_or(1) as f32 / self.fps,
            random: (frame.index as u32)
                .wrapping_mul(2_654_435_761)
                .wrapping_add(0x9e37_79b9),
            view: self.view,
            consecutive,
        };
        let plan = self.plan(targets, &clock)?;
        for (offset, words) in &plan.uniform_writes {
            let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
            self.queue.write_buffer(&self.uniforms, *offset, &bytes);
        }
        self.encode(&mut encoder, &plan)?;

        let final_picture = targets.picture(plan.current);
        let down = self.bind_group(
            &self.down,
            &[
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&final_picture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&targets.out.view),
                },
            ],
        );
        dispatch(&mut encoder, &self.down, &down, width, height, (8, 8));
        final_picture.copy_to(&mut encoder, &targets.previous);
        encoder.copy_texture_to_buffer(
            targets.out.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &targets.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(targets.row_bytes),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let rgba = self.read_back(targets)?;
        self.take_errors()?;
        self.last_frame = Some(frame.index);
        Ok(Frame {
            index: frame.index,
            width,
            height,
            rgba,
        })
    }

    /// Room in the uniform buffer for every pass of every instance.
    fn grow_uniforms(&mut self) {
        let total_passes: usize = self
            .instances
            .iter()
            .map(|i| self.effects[&i.effect].passes.len())
            .sum();
        if total_passes as u64 > self.uniform_slots {
            self.uniform_slots = (total_passes as u64).next_power_of_two();
            self.uniforms = Self::uniform_buffer(&self.device, self.uniform_slots);
        }
    }

    fn upload(&self, targets: &Targets, frame: &Frame) {
        let (width, height) = (targets.width, targets.height);
        self.queue.write_texture(
            targets.upload.texture.as_image_copy(),
            &frame.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// A frame that does not follow the last one starts with no previous
    /// picture and empty persistent textures.
    fn clear_history(&self, encoder: &mut wgpu::CommandEncoder, targets: &Targets) {
        clear(encoder, &targets.previous);
        for inst in &self.instances {
            for slot in &inst.slots {
                if slot.persistent {
                    clear(encoder, &slot.tex);
                    if let Some(shadow) = &slot.shadow {
                        clear(encoder, shadow);
                    }
                }
            }
        }
    }

    /// The rendered frame, read back from the GPU without the row padding.
    fn read_back(&self, targets: &Targets) -> Result<Vec<u8>, String> {
        let (width, height) = (targets.width, targets.height);
        let slice = targets.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        {
            let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
            for row in 0..height as usize {
                let start = row * targets.row_bytes as usize;
                rgba.extend_from_slice(&data[start..start + (width * 4) as usize]);
            }
        }
        targets.readback.unmap();
        Ok(rgba)
    }
}
