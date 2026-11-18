use swofty_shaders::TextureDecl;

use crate::gpu::{BYTE, PICTURE, Preview};

pub(super) struct Tex {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) store: wgpu::TextureView,
    pub(super) levels: Vec<wgpu::TextureView>,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) mips: u32,
}

impl Tex {
    pub(super) fn new(
        device: &wgpu::Device,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self::with_mips(device, label, width, height, format, 1)
    }

    pub(super) fn with_mips(
        device: &wgpu::Device,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        mips: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let level = |mip: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let store = level(0);
        let levels = if mips > 1 {
            (0..mips).map(level).collect()
        } else {
            Vec::new()
        };
        Self {
            texture,
            view,
            store,
            levels,
            width,
            height,
            mips,
        }
    }

    pub(super) fn copy_to(&self, encoder: &mut wgpu::CommandEncoder, to: &Tex) {
        for mip in 0..self.mips.min(to.mips) {
            encoder.copy_texture_to_texture(
                level_of(&self.texture, mip),
                level_of(&to.texture, mip),
                wgpu::Extent3d {
                    width: (self.width >> mip).max(1),
                    height: (self.height >> mip).max(1),
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

fn level_of(texture: &wgpu::Texture, mip_level: u32) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
    }
}

pub(super) fn clear(encoder: &mut wgpu::CommandEncoder, tex: &Tex) {
    encoder.clear_texture(&tex.texture, &wgpu::ImageSubresourceRange::default());
}

pub(super) fn format_of(_decl: &TextureDecl) -> wgpu::TextureFormat {
    PICTURE
}

/// Declared textures are all rgba16float here. After a pass writes one declared with another format, the
/// kernel this names gives it the values the game's texture of that format holds: clamped and rounded
/// for the unorm formats, missing channels as 0 (alpha 1), no negatives for r11g11b10f.
pub(super) const NARROW: [&str; 5] = ["as_rgba8", "as_rgb10a2", "as_rg", "as_r", "as_r11g11b10f"];

pub(super) fn narrow_of(decl: &TextureDecl) -> Option<&'static str> {
    match decl.format.as_deref()? {
        "rgba8" => Some(NARROW[0]),
        "rgb10a2" => Some(NARROW[1]),
        "rg16f" => Some(NARROW[2]),
        "r16f" | "r32f" => Some(NARROW[3]),
        "r11g11b10f" => Some(NARROW[4]),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Picture {
    Original,
    A,
    B,
}

pub(super) struct Targets {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) upload: Tex,
    pub(super) original: Tex,
    pub(super) previous: Tex,
    pictures: [Tex; 2],
    pub(super) out: Tex,
    pub(super) readback: wgpu::Buffer,
    pub(super) row_bytes: u32,
}

impl Preview {
    pub(super) fn ensure_targets(&mut self, width: u32, height: u32) {
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.width == width && t.height == height)
        {
            return;
        }
        let row_bytes = (width * 4).div_ceil(256) * 256;
        self.targets = Some(Targets {
            width,
            height,
            upload: Tex::new(&self.device, "upload", width, height, BYTE),
            original: Tex::new(&self.device, "original", width, height, PICTURE),
            previous: Tex::new(&self.device, "previous", width, height, PICTURE),
            pictures: [
                Tex::new(&self.device, "picture a", width, height, PICTURE),
                Tex::new(&self.device, "picture b", width, height, PICTURE),
            ],
            out: Tex::new(&self.device, "out", width, height, BYTE),
            readback: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: u64::from(row_bytes) * u64::from(height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            row_bytes,
        });
        self.instances_dirty = true;
        self.last_frame = None;
    }
}

impl Picture {
    /// The ping-pong picture a pass that writes the picture writes into.
    pub(super) fn other(self) -> Picture {
        if self == Picture::A {
            Picture::B
        } else {
            Picture::A
        }
    }
}

impl Targets {
    pub(super) fn picture(&self, p: Picture) -> &Tex {
        match p {
            Picture::Original => &self.original,
            Picture::A => &self.pictures[0],
            Picture::B => &self.pictures[1],
        }
    }
}
