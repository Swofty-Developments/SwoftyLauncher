use std::num::NonZeroU64;

use crate::gpu::textures::{Picture, Targets};
use crate::gpu::{INPUTS, Preview, UNIFORM_BYTES, UNIFORM_DWORDS, UNIFORM_STRIDE};

impl Preview {
    pub(super) fn bind_group(
        &self,
        pipeline: &wgpu::ComputePipeline,
        entries: &[wgpu::BindGroupEntry],
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries,
        })
    }

    /// Every pass of every instance in order: its uniforms, its bind group, and
    /// the copies, narrowing and mip chains around it.
    pub(super) fn plan(&self, targets: &Targets, clock: &Clock) -> Result<Plan, String> {
        let (width, height) = (targets.width, targets.height);
        let mut plan = Plan {
            current: Picture::Original,
            uniform_writes: Vec::new(),
            bind_groups: Vec::new(),
            steps: Vec::new(),
        };
        let mut global_pass = 0u32;
        for (ii, inst) in self.instances.iter().enumerate() {
            let fx = &self.effects[&inst.effect];
            for (pi, (decl, pipe)) in fx.effect.passes.iter().zip(&fx.passes).enumerate() {
                let slot_of = |name: &str| inst.slots.iter().find(|s| s.name == name);
                let writes_picture = decl.output.is_none();
                let output = match &decl.output {
                    None => &targets.picture(plan.current.other()).view,
                    Some(name) => {
                        &slot_of(name)
                            .ok_or_else(|| format!("effect {}: no texture {name}", inst.effect))?
                            .tex
                            .store
                    }
                };
                let (out_w, out_h) = match &decl.output {
                    None => (width, height),
                    Some(name) => {
                        let s = slot_of(name).ok_or("missing texture")?;
                        (s.tex.width, s.tex.height)
                    }
                };
                let mut inputs: Vec<&wgpu::TextureView> = Vec::new();
                for name in &decl.inputs {
                    let view = match name.as_str() {
                        "original" => &targets.original.view,
                        "previous" => &targets.previous.view,
                        other => {
                            if let Some(s) = slot_of(other) {
                                if decl.output.as_deref() == Some(other) {
                                    match &s.shadow {
                                        Some(shadow) => {
                                            plan.steps.push(Step::Shadow(ii, other.to_string()));
                                            &shadow.view
                                        }
                                        None => &s.tex.view,
                                    }
                                } else {
                                    &s.tex.view
                                }
                            } else if let Some(image) = fx.images.get(other) {
                                &image.view
                            } else {
                                return Err(format!(
                                    "effect {}: pass {} reads unknown texture {other}",
                                    inst.effect, decl.name
                                ));
                            }
                        }
                    };
                    inputs.push(view);
                }
                if inputs.len() > INPUTS {
                    return Err(format!(
                        "effect {}: pass {} has more than {INPUTS} inputs",
                        inst.effect, decl.name
                    ));
                }

                let scene = targets.picture(plan.current);
                let offset = u64::from(global_pass) * UNIFORM_STRIDE;
                plan.uniform_writes.push((
                    offset,
                    pass_uniforms(
                        clock,
                        (out_w, out_h),
                        (scene.width, scene.height),
                        global_pass,
                        &inst.params,
                    ),
                ));

                let mut entries = vec![
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.uniforms,
                            offset,
                            size: NonZeroU64::new(UNIFORM_BYTES),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::TextureView(&scene.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: wgpu::BindingResource::TextureView(&self.depth.view),
                    },
                ];
                for i in 0..INPUTS {
                    entries.push(wgpu::BindGroupEntry {
                        binding: 12 + i as u32,
                        resource: wgpu::BindingResource::TextureView(
                            inputs.get(i).copied().unwrap_or(&self.dummy.view),
                        ),
                    });
                }
                entries.push(wgpu::BindGroupEntry {
                    binding: 30,
                    resource: wgpu::BindingResource::TextureView(output),
                });
                for (i, s) in self.samplers.iter().enumerate() {
                    entries.push(wgpu::BindGroupEntry {
                        binding: 40 + i as u32,
                        resource: wgpu::BindingResource::Sampler(s),
                    });
                }
                let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&decl.name),
                    layout: &pipe.layout,
                    entries: &entries,
                });
                plan.bind_groups.push((ii, pi, bind, out_w, out_h));
                plan.steps.push(Step::Pass(plan.bind_groups.len() - 1));
                if let Some(name) = &decl.output
                    && slot_of(name).is_some_and(|s| s.narrow.is_some())
                {
                    plan.steps.push(Step::Narrow(ii, name.clone()));
                }
                if let Some(name) = &decl.output
                    && slot_of(name).is_some_and(|s| s.tex.mips > 1)
                {
                    plan.steps.push(Step::Mips(ii, name.clone()));
                }
                if writes_picture {
                    plan.current = plan.current.other();
                }
                global_pass += 1;
            }
        }
        Ok(plan)
    }

    pub(super) fn encode(&self, encoder: &mut wgpu::CommandEncoder, plan: &Plan) -> Result<(), String> {
        for step in &plan.steps {
            match step {
                Step::Shadow(ii, name) => {
                    let slot = self.slot(*ii, name)?;
                    if let Some(shadow) = &slot.shadow {
                        slot.tex.copy_to(encoder, shadow);
                    }
                }
                Step::Pass(bi) => {
                    let (ii, pi, bind, w, h) = &plan.bind_groups[*bi];
                    let fx = &self.effects[&self.instances[*ii].effect];
                    let pipe = &fx.passes[*pi];
                    dispatch(encoder, &pipe.pipeline, bind, *w, *h, pipe.threads);
                }
                Step::Mips(ii, name) => {
                    let slot = self.slot(*ii, name)?;
                    for level in 1..slot.tex.mips as usize {
                        let bind = self.bind_group(
                            &self.mip,
                            &[
                                wgpu::BindGroupEntry {
                                    binding: 6,
                                    resource: wgpu::BindingResource::TextureView(&slot.tex.levels[level - 1]),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 7,
                                    resource: wgpu::BindingResource::TextureView(&slot.tex.levels[level]),
                                },
                            ],
                        );
                        dispatch(
                            encoder,
                            &self.mip,
                            &bind,
                            (slot.tex.width >> level).max(1),
                            (slot.tex.height >> level).max(1),
                            (8, 8),
                        );
                    }
                }
                Step::Narrow(ii, name) => {
                    let slot = self.slot(*ii, name)?;
                    if let Some((entry, scratch)) = &slot.narrow {
                        slot.tex.copy_to(encoder, scratch);
                        let pipe = &self.narrow[entry];
                        let bind = self.bind_group(
                            pipe,
                            &[
                                wgpu::BindGroupEntry {
                                    binding: 4,
                                    resource: wgpu::BindingResource::TextureView(&scratch.view),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 5,
                                    resource: wgpu::BindingResource::TextureView(&slot.tex.store),
                                },
                            ],
                        );
                        dispatch(encoder, pipe, &bind, slot.tex.width, slot.tex.height, (8, 8));
                    }
                }
            }
        }
        Ok(())
    }
}

pub(super) fn dispatch(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::ComputePipeline,
    bind: &wgpu::BindGroup,
    w: u32,
    h: u32,
    threads: (u32, u32),
) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, &[]);
    pass.dispatch_workgroups(w.div_ceil(threads.0), h.div_ceil(threads.1), 1);
}

/// The frame's clock and view, the same for every pass of the frame.
pub(super) struct Clock {
    pub(super) index: usize,
    pub(super) time: f32,
    pub(super) frametime: f32,
    pub(super) random: u32,
    pub(super) view: u32,
    pub(super) consecutive: bool,
}

/// One pass's uniform block, in the layout of the shaders' constant buffer.
fn pass_uniforms(
    clock: &Clock,
    (out_w, out_h): (u32, u32),
    (scene_w, scene_h): (u32, u32),
    global_pass: u32,
    params: &[f32],
) -> [u32; UNIFORM_DWORDS] {
    let mut u = [0u32; UNIFORM_DWORDS];
    u[0] = out_w;
    u[1] = out_h;
    u[2] = (1.0 / out_w as f32).to_bits();
    u[3] = (1.0 / out_h as f32).to_bits();
    u[4] = clock.time.to_bits();
    u[5] = clock.index as u32;
    u[6] = 0;
    u[7] = global_pass;
    for (i, v) in params.iter().take(32).enumerate() {
        if i < 16 {
            u[8 + i] = v.to_bits();
        } else {
            u[36 + (i - 16)] = v.to_bits();
        }
    }
    u[24] = 1;
    u[25] = 1;
    u[26] = 0;
    u[27] = clock.frametime.to_bits();
    u[28] = scene_w;
    u[29] = scene_h;
    u[30] = clock.random;
    u[31] = clock.view;
    u[32] = 0;
    u[33] = u32::from(clock.view != 0);
    u[34] = u32::from(clock.consecutive);
    u
}

/// A frame's passes, planned before any of them is encoded.
pub(super) struct Plan {
    pub(super) current: Picture,
    pub(super) uniform_writes: Vec<(u64, [u32; UNIFORM_DWORDS])>,
    bind_groups: Vec<(usize, usize, wgpu::BindGroup, u32, u32)>,
    steps: Vec<Step>,
}

enum Step {
    Mips(usize, String),
    Shadow(usize, String),
    Pass(usize),
    Narrow(usize, String),
}
