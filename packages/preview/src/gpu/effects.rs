use std::collections::HashMap;

use swofty_shaders::mip_levels;

use crate::gpu::textures::{Tex, format_of, narrow_of};
use crate::gpu::{BYTE, INPUTS, PICTURE, Preview, scaled};

pub(super) struct PassPipe {
    pub(super) pipeline: wgpu::ComputePipeline,
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) threads: (u32, u32),
}

pub(super) struct EffectGpu {
    pub(super) effect: swofty_shaders::Effect,
    pub(super) passes: Vec<PassPipe>,
    pub(super) images: HashMap<String, Tex>,
}

pub(super) struct Slot {
    pub(super) name: String,
    pub(super) tex: Tex,
    pub(super) shadow: Option<Tex>,
    pub(super) narrow: Option<(&'static str, Tex)>,
    pub(super) persistent: bool,
}

pub(super) struct Inst {
    pub(super) effect: String,
    pub(super) params: Vec<f32>,
    pub(super) slots: Vec<Slot>,
}

pub(super) fn sampler(
    device: &wgpu::Device,
    filter: wgpu::FilterMode,
    address: wgpu::AddressMode,
) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: match filter {
            wgpu::FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
            _ => wgpu::MipmapFilterMode::Nearest,
        },
        ..Default::default()
    })
}

pub(super) fn layout_for(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let sampled = |binding: u32, filterable: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let sampler = |binding: u32, filtering: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Sampler(if filtering {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        }),
        count: None,
    };
    let mut entries = vec![
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        sampled(10, true),
        sampled(11, false),
    ];
    for i in 0..INPUTS as u32 {
        entries.push(sampled(12 + i, true));
    }
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 30,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::ReadWrite,
            format: PICTURE,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    });
    entries.push(sampler(40, true));
    entries.push(sampler(41, false));
    entries.push(sampler(42, true));
    entries.push(sampler(43, false));
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("swofty pass"),
        entries: &entries,
    })
}

pub(super) fn words(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if !bytes.len().is_multiple_of(4) || bytes.is_empty() {
        return Err("a SPIR-V blob is not a whole number of words".into());
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

impl Preview {
    fn build_effect(&self, id: &str) -> Result<EffectGpu, String> {
        let source = self
            .sources
            .get(id)
            .ok_or_else(|| format!("effect {id} is not loaded in the preview"))?;
        let effect = &source.effect;
        if source.blobs.len() != effect.passes.len() {
            return Err(format!(
                "effect {id}: {} blobs for {} passes",
                source.blobs.len(),
                effect.passes.len()
            ));
        }
        let mut passes = Vec::new();
        for (decl, blob) in effect.passes.iter().zip(&source.blobs) {
            let spirv = words(blob).map_err(|e| format!("effect {id}, pass {}: {e}", decl.name))?;
            let threads = decl.threads.map_or((8, 8), |t| (t[0].max(1), t[1].max(1)));
            let label = format!("{id}/{}", decl.name);
            let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&label),
                source: wgpu::ShaderSource::SpirV(spirv.into()),
            });
            if let Some(name) = &decl.output
                && !effect.textures.iter().any(|t| &t.name == name)
            {
                return Err(format!("effect {id}, pass {}: no texture {name}", decl.name));
            }
            let layout = self.layout.clone();
            let pipeline_layout = self
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
            let pipeline = self
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(&format!("{id}/{}", decl.name)),
                    layout: Some(&pipeline_layout),
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
            self.take_errors()
                .map_err(|e| format!("effect {id}, pass {}: {e}", decl.name))?;
            passes.push(PassPipe {
                pipeline,
                layout,
                threads,
            });
        }
        let mut images = HashMap::new();
        for (name, w, h, rgba) in &source.images {
            let tex = Tex::new(&self.device, name, *w, *h, BYTE);
            self.queue.write_texture(
                tex.texture.as_image_copy(),
                rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                },
            );
            images.insert(name.clone(), tex);
        }
        Ok(EffectGpu {
            effect: effect.clone(),
            passes,
            images,
        })
    }

    pub(super) fn build_instances(&mut self, width: u32, height: u32) -> Result<(), String> {
        for request in &self.requested {
            if !self.effects.contains_key(&request.effect) {
                let built = self.build_effect(&request.effect)?;
                self.effects.insert(request.effect.clone(), built);
            }
        }
        let mut instances = Vec::new();
        for request in &self.requested {
            let fx = &self.effects[&request.effect];
            let mut slots = Vec::new();
            for decl in fx.effect.textures.iter().filter(|t| t.file.is_none()) {
                let (w, h) = match (decl.width, decl.height, decl.scale) {
                    (Some(w), Some(h), _) => (w.max(1), h.max(1)),
                    (_, _, Some(s)) => (scaled(width, s), scaled(height, s)),
                    _ => (width, height),
                };
                let format = format_of(decl);
                let mips = mip_levels(w, h, decl.mips.unwrap_or(1));
                let shadow = fx.effect.passes.iter().any(|p| {
                    p.output.as_deref() == Some(decl.name.as_str()) && p.inputs.contains(&decl.name)
                });
                slots.push(Slot {
                    name: decl.name.clone(),
                    tex: Tex::with_mips(&self.device, &decl.name, w, h, format, mips),
                    shadow: shadow.then(|| Tex::with_mips(&self.device, &decl.name, w, h, format, mips)),
                    narrow: narrow_of(decl)
                        .map(|entry| (entry, Tex::new(&self.device, &decl.name, w, h, format))),
                    persistent: decl.persistent,
                });
            }
            instances.push(Inst {
                effect: request.effect.clone(),
                params: request.params.clone(),
                slots,
            });
        }
        self.instances = instances;
        self.instances_dirty = false;
        Ok(())
    }

    pub(super) fn slot(&self, ii: usize, name: &str) -> Result<&Slot, String> {
        self.instances[ii]
            .slots
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| "missing texture".to_owned())
    }
}
