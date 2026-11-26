use crate::camera::Camera;
use crate::theme;
use eframe::egui_wgpu::{self, wgpu};
use egui::{Color32, TextureId};
use glam::Mat4;
use modelview_core::{Bounds, DefectKind, Format, MeshReport, Scene, smooth_normals, weld};
use wgpu::util::DeviceExt;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLE_COUNT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    camera: [f32; 4],
    params: [f32; 4],
    focus: [f32; 4],
    focus_color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct Focus {
    pub kind: DefectKind,
    pub center: [f32; 3],
    pub radius: f32,
    pub color: Color32,
    pub amount: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ObjectUniform {
    base: [f32; 4],
    flags: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineVertex {
    position: [f32; 3],
    color: [f32; 3],
}

struct GpuObject {
    scene_index: usize,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    base: [f32; 4],
    flags: [f32; 4],
}

struct GpuLines {
    buffer: wgpu::Buffer,
    count: u32,
}

struct Targets {
    size: [u32; 2],
    color_view: wgpu::TextureView,
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

pub struct SceneRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    grid_globals_buffer: wgpu::Buffer,
    grid_bind_group: wgpu::BindGroup,
    mesh_pipeline: wgpu::RenderPipeline,
    grid_pipeline: wgpu::RenderPipeline,
    defect_pipeline: wgpu::RenderPipeline,
    defect_xray_pipeline: wgpu::RenderPipeline,
    objects: Vec<GpuObject>,
    grid: Option<GpuLines>,
    bed_grid: Option<GpuLines>,
    defects: Option<GpuLines>,
    targets: Option<Targets>,
    texture_id: Option<TextureId>,
    selection: Option<usize>,
}

impl SceneRenderer {
    pub fn new(
        render_state: &egui_wgpu::RenderState,
        scene: &Scene,
        reports: &[MeshReport],
        visible: &[bool],
        filters: &[bool; 4],
    ) -> Self {
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();

        let globals = Globals {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            model: Mat4::IDENTITY.to_cols_array_2d(),
            camera: [0.0, 0.0, 0.0, 1.0],
            params: [0.0; 4],
            focus: [0.0; 4],
            focus_color: [0.0; 4],
        };
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let object_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("object_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals_bind_group"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let grid_globals = Globals {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            model: Mat4::IDENTITY.to_cols_array_2d(),
            camera: [0.0, 0.0, 0.0, 1.0],
            params: [0.0; 4],
            focus: [0.0; 4],
            focus_color: [0.0; 4],
        };
        let grid_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid_globals"),
            contents: bytemuck::bytes_of(&grid_globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let grid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid_bind_group"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: grid_globals_buffer.as_entire_binding(),
            }],
        });

        let mesh_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/mesh.wgsl").into()),
        });
        let line_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("line_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/line.wgsl").into()),
        });

        let mesh_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh_pipeline_layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&object_layout)],
            immediate_size: 0,
        });
        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line_pipeline_layout"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });

        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh_pipeline"),
            layout: Some(&mesh_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let line_pipeline = |label: &str,
                             write: bool,
                             bias: wgpu::DepthBiasState,
                             compare: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&line_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &line_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<LineVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::LineList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(compare),
                    stencil: wgpu::StencilState::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &line_shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let grid_pipeline = line_pipeline(
            "grid_pipeline",
            true,
            wgpu::DepthBiasState::default(),
            wgpu::CompareFunction::LessEqual,
        );
        let defect_pipeline = line_pipeline(
            "defect_pipeline",
            false,
            wgpu::DepthBiasState::default(),
            wgpu::CompareFunction::LessEqual,
        );
        let defect_xray_pipeline = line_pipeline(
            "defect_xray_pipeline",
            false,
            wgpu::DepthBiasState::default(),
            wgpu::CompareFunction::Always,
        );

        let palette = [
            Color32::from_rgb(0xd3, 0xdc, 0xe6),
            Color32::from_rgb(0x9d, 0xb2, 0xc8),
            Color32::from_rgb(0xc2, 0xcc, 0xd8),
            Color32::from_rgb(0x78, 0x91, 0xa8),
        ];

        let mut objects = Vec::new();
        for (scene_index, object) in scene.objects.iter().enumerate() {
            if object.mesh.is_empty() {
                continue;
            }
            let render_mesh = if scene.format == Format::Stl {
                weld(&object.mesh)
            } else {
                object.mesh.clone()
            };
            let normals = smooth_normals(&render_mesh.positions, &render_mesh.indices);
            let vertices: Vec<Vertex> = render_mesh
                .positions
                .iter()
                .zip(normals.iter())
                .map(|(p, n)| Vertex {
                    position: *p,
                    normal: *n,
                })
                .collect();
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("object_vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let indices: Vec<u32> = render_mesh
                .indices
                .iter()
                .flat_map(|t| [t[0], t[1], t[2]])
                .collect();
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("object_indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let base = object
                .color
                .map(display_part_color)
                .unwrap_or_else(|| palette[scene_index % palette.len()]);
            let base = linear_rgba(base);
            let flags = [0.0, 0.0, 0.0, 0.0];
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("object_uniform"),
                contents: bytemuck::bytes_of(&ObjectUniform { base, flags }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("object_bind_group"),
                layout: &object_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            objects.push(GpuObject {
                scene_index,
                vertex_buffer,
                index_buffer,
                index_count: indices.len() as u32,
                uniform,
                bind_group,
                base,
                flags,
            });
        }

        let grid = build_grid(&scene.bounds()).map(|data| GpuLines {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("grid"),
                contents: bytemuck::cast_slice(&data),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            count: data.len() as u32,
        });
        let bed_grid = build_bed_grid().map(|data| GpuLines {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("bed_grid"),
                contents: bytemuck::cast_slice(&data),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            count: data.len() as u32,
        });

        let mut renderer = Self {
            device,
            queue,
            globals_buffer,
            globals_bind_group,
            grid_globals_buffer,
            grid_bind_group,
            mesh_pipeline,
            grid_pipeline,
            defect_pipeline,
            defect_xray_pipeline,
            objects,
            grid,
            bed_grid,
            defects: None,
            targets: None,
            texture_id: None,
            selection: None,
        };
        renderer.rebuild_defects(reports, visible, filters, None);
        renderer
    }

    pub fn rebuild_defects(
        &mut self,
        reports: &[MeshReport],
        visible: &[bool],
        filters: &[bool; 4],
        focus: Option<Focus>,
    ) {
        let mut vertices: Vec<LineVertex> = Vec::new();
        for (index, report) in reports.iter().enumerate() {
            if !visible.get(index).copied().unwrap_or(true) {
                continue;
            }
            for line in &report.defect_lines {
                if !filters[defect_index(line.kind)] {
                    continue;
                }
                let color = srgb_hex(theme::kind_line_color(line.kind));
                let color = if let Some(focus) = focus {
                    let midpoint = [
                        (line.a[0] + line.b[0]) * 0.5,
                        (line.a[1] + line.b[1]) * 0.5,
                        (line.a[2] + line.b[2]) * 0.5,
                    ];
                    let dx = midpoint[0] - focus.center[0];
                    let dy = midpoint[1] - focus.center[1];
                    let dz = midpoint[2] - focus.center[2];
                    let local =
                        dx * dx + dy * dy + dz * dz <= (focus.radius * 1.8) * (focus.radius * 1.8);
                    if line.kind == focus.kind && local {
                        color
                    } else {
                        [
                            color[0] * 0.18 + 0.004,
                            color[1] * 0.18 + 0.006,
                            color[2] * 0.18 + 0.009,
                        ]
                    }
                } else {
                    color
                };
                vertices.push(LineVertex {
                    position: line.a,
                    color,
                });
                vertices.push(LineVertex {
                    position: line.b,
                    color,
                });
            }
        }
        if vertices.is_empty() {
            self.defects = None;
            return;
        }
        let count = vertices.len() as u32;
        self.defects = Some(GpuLines {
            buffer: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("defect_lines"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            count,
        });
    }

    pub fn set_selection(&mut self, selection: Option<usize>) {
        self.selection = selection;
    }

    fn ensure_targets(&mut self, egui_renderer: &mut egui_wgpu::Renderer, size: [u32; 2]) {
        if let Some(targets) = &self.targets
            && targets.size == size
        {
            return;
        }
        let max = self.device.limits().max_texture_dimension_2d.min(8192);
        let size = [size[0].clamp(1, max), size[1].clamp(1, max)];
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport_color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport_msaa"),
            size: extent,
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport_depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        match self.texture_id {
            Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                &self.device,
                &color_view,
                wgpu::FilterMode::Linear,
                id,
            ),
            None => {
                self.texture_id = Some(egui_renderer.register_native_texture(
                    &self.device,
                    &color_view,
                    wgpu::FilterMode::Linear,
                ));
            }
        }
        self.targets = Some(Targets {
            size,
            color_view,
            msaa_view,
            depth_view,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        render_state: &egui_wgpu::RenderState,
        camera: &Camera,
        size: [u32; 2],
        aspect: f32,
        visible: &[bool],
        grid: bool,
        bed: bool,
        defects: bool,
        model: Mat4,
        focus: Option<Focus>,
    ) -> Option<TextureId> {
        {
            let mut egui_renderer = render_state.renderer.write();
            self.ensure_targets(&mut egui_renderer, size);
        }
        let targets = self.targets.as_ref()?;

        let eye = camera.eye();
        let (focus_data, focus_color) = match focus {
            Some(focus) => {
                let color = linear(focus.color);
                (
                    [
                        focus.center[0],
                        focus.center[1],
                        focus.center[2],
                        focus.radius,
                    ],
                    [color[0], color[1], color[2], focus.amount],
                )
            }
            None => ([0.0; 4], [0.0; 4]),
        };
        let globals = Globals {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            camera: [eye.x, eye.y, eye.z, 1.0],
            params: [0.0; 4],
            focus: focus_data,
            focus_color,
        };
        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        let grid_globals = Globals {
            view_proj: globals.view_proj,
            model: Mat4::IDENTITY.to_cols_array_2d(),
            camera: globals.camera,
            params: [0.0; 4],
            focus: [0.0; 4],
            focus_color: [0.0; 4],
        };
        self.queue.write_buffer(
            &self.grid_globals_buffer,
            0,
            bytemuck::bytes_of(&grid_globals),
        );

        for object in &mut self.objects {
            let selected = self.selection == Some(object.scene_index);
            let flags = if selected {
                [1.0, 0.0, 0.0, 0.0]
            } else {
                [0.0; 4]
            };
            if flags != object.flags {
                object.flags = flags;
                self.queue.write_buffer(
                    &object.uniform,
                    0,
                    bytemuck::bytes_of(&ObjectUniform {
                        base: object.base,
                        flags,
                    }),
                );
            }
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene_encoder"),
            });
        {
            let clear = theme::VOID;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa_view,
                    resolve_target: Some(&targets.color_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear.r() as f64 / 255.0,
                            g: clear.g() as f64 / 255.0,
                            b: clear.b() as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            if bed && let Some(grid) = &self.bed_grid {
                pass.set_bind_group(0, &self.grid_bind_group, &[]);
                pass.set_pipeline(&self.grid_pipeline);
                pass.set_vertex_buffer(0, grid.buffer.slice(..));
                pass.draw(0..grid.count, 0..1);
            } else if grid && let Some(grid) = &self.grid {
                pass.set_bind_group(0, &self.grid_bind_group, &[]);
                pass.set_pipeline(&self.grid_pipeline);
                pass.set_vertex_buffer(0, grid.buffer.slice(..));
                pass.draw(0..grid.count, 0..1);
            }
            pass.set_bind_group(0, &self.globals_bind_group, &[]);

            pass.set_pipeline(&self.mesh_pipeline);
            for object in &self.objects {
                if !visible.get(object.scene_index).copied().unwrap_or(true) {
                    continue;
                }
                pass.set_bind_group(1, &object.bind_group, &[]);
                pass.set_vertex_buffer(0, object.vertex_buffer.slice(..));
                pass.set_index_buffer(object.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..object.index_count, 0, 0..1);
            }

            if defects && let Some(defects) = &self.defects {
                let xray = focus.is_some_and(|focus| focus.amount > 0.001);
                pass.set_pipeline(if xray {
                    &self.defect_xray_pipeline
                } else {
                    &self.defect_pipeline
                });
                pass.set_vertex_buffer(0, defects.buffer.slice(..));
                pass.draw(0..defects.count, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.texture_id
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LiveGlobals {
    view_proj: [[f32; 4]; 4],
    camera: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LiveVertex {
    position: [f32; 3],
}

pub struct LiveRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
    targets: Option<Targets>,
    texture_id: Option<TextureId>,
}

impl LiveRenderer {
    pub fn new(render_state: &egui_wgpu::RenderState, capacity_vertices: usize) -> Option<Self> {
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let vertex_bytes = capacity_vertices
            .max(3)
            .checked_mul(std::mem::size_of::<LiveVertex>())?;
        if vertex_bytes as u64 > device.limits().max_buffer_size {
            return None;
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("live_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/live.wgsl").into()),
        });
        let globals = LiveGlobals {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            camera: [0.0, 0.0, 0.0, 1.0],
            params: [0.0; 4],
        };
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("live_globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("live_globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("live_globals_bind_group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("live_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("live_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LiveVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("live_vertices"),
            size: vertex_bytes as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(Self {
            device,
            queue,
            globals_buffer,
            globals_bind_group,
            pipeline,
            vertex_buffer,
            vertex_capacity: vertex_bytes / std::mem::size_of::<LiveVertex>(),
            vertex_count: 0,
            targets: None,
            texture_id: None,
        })
    }

    pub fn append(&mut self, positions: &[[f32; 3]]) -> bool {
        if self.vertex_count + positions.len() > self.vertex_capacity {
            return false;
        }
        if positions.is_empty() {
            return true;
        }
        let vertices: Vec<LiveVertex> = positions
            .iter()
            .map(|position| LiveVertex {
                position: *position,
            })
            .collect();
        self.queue.write_buffer(
            &self.vertex_buffer,
            (self.vertex_count * std::mem::size_of::<LiveVertex>()) as u64,
            bytemuck::cast_slice(&vertices),
        );
        self.vertex_count += vertices.len();
        true
    }

    fn ensure_targets(&mut self, egui_renderer: &mut egui_wgpu::Renderer, size: [u32; 2]) {
        if let Some(targets) = &self.targets
            && targets.size == size
        {
            return;
        }
        let max = self.device.limits().max_texture_dimension_2d.min(8192);
        let size = [size[0].clamp(1, max), size[1].clamp(1, max)];
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("live_color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("live_msaa"),
            size: extent,
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("live_depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        match self.texture_id {
            Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                &self.device,
                &color_view,
                wgpu::FilterMode::Linear,
                id,
            ),
            None => {
                self.texture_id = Some(egui_renderer.register_native_texture(
                    &self.device,
                    &color_view,
                    wgpu::FilterMode::Linear,
                ));
            }
        }
        self.targets = Some(Targets {
            size,
            color_view,
            msaa_view,
            depth_view,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        render_state: &egui_wgpu::RenderState,
        camera: &Camera,
        size: [u32; 2],
        aspect: f32,
    ) -> Option<TextureId> {
        {
            let mut egui_renderer = render_state.renderer.write();
            self.ensure_targets(&mut egui_renderer, size);
        }
        let targets = self.targets.as_ref()?;
        if self.vertex_count == 0 {
            return self.texture_id;
        }
        let eye = camera.eye();
        let globals = LiveGlobals {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            camera: [eye.x, eye.y, eye.z, 1.0],
            params: [0.0; 4],
        };
        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("live_encoder"),
            });
        {
            let clear = theme::VOID;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("live_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa_view,
                    resolve_target: Some(&targets.color_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear.r() as f64 / 255.0,
                            g: clear.g() as f64 / 255.0,
                            b: clear.b() as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.vertex_count as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        self.texture_id
    }
}

fn defect_index(kind: DefectKind) -> usize {
    match kind {
        DefectKind::OpenEdge => 0,
        DefectKind::NonManifold => 1,
        DefectKind::InconsistentWinding => 2,
        DefectKind::DegenerateFace => 3,
    }
}

pub fn defect_slot(kind: DefectKind) -> usize {
    defect_index(kind)
}

pub fn nice_step(target: f32) -> f32 {
    if !target.is_finite() || target <= 0.0 {
        return 10.0;
    }
    let exp = target.log10().floor();
    let base = 10f32.powf(exp);
    let scaled = target / base;
    let step = if scaled < 1.5 {
        1.0
    } else if scaled < 3.5 {
        2.0
    } else if scaled < 7.5 {
        5.0
    } else {
        10.0
    };
    (step * base).max(0.1)
}

fn build_bed_grid() -> Option<Vec<LineVertex>> {
    let half = 128.0_f32;
    let z = -0.02;
    let minor = srgb_hex(Color32::from_rgb(0x10, 0x1a, 0x24));
    let major = srgb_hex(Color32::from_rgb(0x1b, 0x2a, 0x3a));
    let axis = srgb_hex(Color32::from_rgb(0x1f, 0x4c, 0x8f));
    let mut vertices = Vec::with_capacity(64);
    for index in -4..=4 {
        let position = index as f32 * 32.0;
        let color = if index == 0 {
            axis
        } else if index % 2 == 0 {
            major
        } else {
            minor
        };
        vertices.push(LineVertex {
            position: [position, -half, z],
            color,
        });
        vertices.push(LineVertex {
            position: [position, half, z],
            color,
        });
        vertices.push(LineVertex {
            position: [-half, position, z],
            color,
        });
        vertices.push(LineVertex {
            position: [half, position, z],
            color,
        });
    }
    Some(vertices)
}

fn build_grid(bounds: &Bounds) -> Option<Vec<LineVertex>> {
    let size = bounds.size();
    let span = size.x.max(size.y).max(20.0);
    let step = nice_step(span / 10.0);
    let radius = ((span * 1.25 * 0.5) / step).ceil().clamp(4.0, 48.0) as i32;
    let center = bounds.center();
    let cx = (center.x / step).round() * step;
    let cy = (center.y / step).round() * step;
    let z = if bounds.is_empty() {
        -0.05
    } else {
        bounds.min.z - (span * 0.0004).max(0.02)
    };
    let minor = srgb_hex(Color32::from_rgb(0x10, 0x1a, 0x24));
    let major = srgb_hex(Color32::from_rgb(0x1b, 0x2a, 0x3a));
    let axis = srgb_hex(Color32::from_rgb(0x1f, 0x4c, 0x8f));
    let mut vertices = Vec::with_capacity(((radius * 2 + 1) * 8) as usize);
    for i in -radius..=radius {
        let position = i as f32 * step;
        let color = if i == 0 {
            axis
        } else if i % 5 == 0 {
            major
        } else {
            minor
        };
        let x = cx + position;
        let y = cy + position;
        let extent = radius as f32 * step;
        vertices.push(LineVertex {
            position: [x, cy - extent, z],
            color,
        });
        vertices.push(LineVertex {
            position: [x, cy + extent, z],
            color,
        });
        vertices.push(LineVertex {
            position: [cx - extent, y, z],
            color,
        });
        vertices.push(LineVertex {
            position: [cx + extent, y, z],
            color,
        });
    }
    Some(vertices)
}

fn srgb_to_linear(value: u8) -> f32 {
    let c = value as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear(color: Color32) -> [f32; 3] {
    [
        srgb_to_linear(color.r()),
        srgb_to_linear(color.g()),
        srgb_to_linear(color.b()),
    ]
}

fn display_part_color(rgb: [u8; 3]) -> Color32 {
    let lift = |value: u8| {
        let value = value as f32;
        (value + (255.0 - value) * 0.32).round() as u8
    };
    Color32::from_rgb(lift(rgb[0]), lift(rgb[1]), lift(rgb[2]))
}

fn linear_rgba(color: Color32) -> [f32; 4] {
    let [r, g, b] = linear(color);
    [r, g, b, color.a() as f32 / 255.0]
}

pub fn srgb_hex(color: Color32) -> [f32; 3] {
    [
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
    ]
}
