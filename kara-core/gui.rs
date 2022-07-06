use std::sync::{mpsc, Arc};

use iced_wgpu::{
    wgpu::{
        self,
        util::{
            backend_bits_from_env, initialize_adapter_from_env_or_default, DeviceExt, StagingBelt,
        },
        Backends, CommandEncoderDescriptor, DeviceDescriptor, Features, Instance, Limits,
        PresentMode, SurfaceConfiguration, SurfaceError, TextureUsages, TextureViewDescriptor,
    },
    Backend, Renderer, Settings, Viewport,
};
use iced_winit::{
    conversion,
    futures::{
        executor::{self, LocalPool},
        task::SpawnExt,
    },
    program,
    winit::{
        dpi::PhysicalPosition,
        event::{Event, ModifiersState, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
    },
    Clipboard, Debug, Size,
};
use kara_audio::{stt_sources::stt_source, Config};
use tokio::runtime::Handle;
use tracing::{error, trace};

use crate::config::state::ParsedConfig;

use self::{controls::Controls, scene::Scene};

pub async fn start(config: &ParsedConfig) -> anyhow::Result<()> {
    let stt_source = stt_source(&config.nlu.stt.source).await?;
    let handle = Handle::current();
    // Create EventLoop with 'String' user events
    let event_loop = EventLoop::with_user_event();
    let proxy = event_loop.create_proxy(); // Sends the user events which we can retrieve in the loop
                                           /* TODO: Create an enum for events?*/
    let stream = kara_audio::visualiser_stream(Config::default(), proxy, stt_source);
    let window = iced_winit::winit::window::WindowBuilder::new()
        .with_transparent(true)
        .build(&event_loop)?;
    window.set_title(&config.window.title);
    window.set_decorations(config.window.decorations);
    let physical_size = window.inner_size();
    let mut viewport = Viewport::with_physical_size(
        iced_winit::Size {
            width: physical_size.width,
            height: physical_size.height,
        },
        window.scale_factor(),
    );
    let mut cursor_position = PhysicalPosition::new(-1.0, -1.0);
    let mut modifiers = ModifiersState::default();
    let mut clipboard = Clipboard::connect(&window);

    // Initialise wgpu
    let default_backend = Backends::PRIMARY;
    let backend = backend_bits_from_env().unwrap_or(default_backend);
    let backend = Arc::new(backend);
    let instance = Instance::new(*backend);
    let instance = Arc::new(instance);
    let surface = unsafe { instance.create_surface(&window) };
    let surface = Arc::new(surface);
    let inner_surface = Arc::clone(&surface);
    let inner_instance = Arc::clone(&instance);
    let inner_backend = Arc::clone(&backend);

    let (format, (device, queue)) = tokio::task::spawn_blocking(move || {
        executor::block_on(async {
            handle
                .spawn(async move {
                    let adapter = initialize_adapter_from_env_or_default(
                        &inner_instance,
                        *inner_backend,
                        Some(&inner_surface),
                    )
                    .await
                    .expect("No suitable GPU adapters found in the system");
                    trace!("using gpu adapter: {:?}", &adapter);
                    let adapter_features = adapter.features();
                    let needed_limits = Limits::default();
                    (
                        inner_surface
                            .get_preferred_format(&adapter)
                            .expect("get preferred format"),
                        adapter
                            .request_device(
                                &DeviceDescriptor {
                                    label: None,
                                    features: adapter_features & Features::default(),
                                    limits: needed_limits,
                                },
                                None,
                            )
                            .await
                            .expect("request device"),
                    )
                })
                .await
                .expect("Task spawned in Tokio executor panicked")
        })
    })
    .await?;
    surface.configure(
        &device,
        &SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: physical_size.width,
            height: physical_size.height,
            present_mode: PresentMode::Mailbox,
        },
    );

    let mut resized = false;

    // Initialise staging belt and local pool
    let mut staging_belt = StagingBelt::new(5 * 1024);
    let mut local_pool = LocalPool::new();

    // Initialise scene
    let scene = Scene::new(&device, format);
    let controls = Controls::new(config.window.opacity);

    // Initialise iced
    let mut debug = Debug::new();
    let mut renderer = Renderer::new(Backend::new(&device, Settings::default(), format));

    let mut state =
        program::State::new(controls, viewport.logical_size(), &mut renderer, &mut debug);

    // Run event_loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        cursor_position = position;
                    }
                    WindowEvent::ModifiersChanged(new_modifiers) => {
                        modifiers = new_modifiers;
                    }
                    WindowEvent::Resized(new_size) => {
                        viewport = Viewport::with_physical_size(
                            Size::new(new_size.width, new_size.height),
                            window.scale_factor(),
                        );

                        resized = true;
                    }
                    WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                    }
                    _ => {}
                }

                // Map window event to iced event
                if let Some(event) =
                    iced_winit::conversion::window_event(&event, window.scale_factor(), modifiers)
                {
                    state.queue_event(event);
                }
            }
            Event::MainEventsCleared => {
                // If there are events pending
                if !state.is_queue_empty() {
                    // We update iced
                    let _ = state.update(
                        viewport.logical_size(),
                        conversion::cursor_position(cursor_position, viewport.scale_factor()),
                        &mut renderer,
                        &mut clipboard,
                        &mut debug,
                    );

                    // and request a redraw
                    window.request_redraw();
                }
            }
            Event::RedrawRequested(_) => {
                if resized {
                    let size = window.inner_size();

                    surface.configure(
                        &device,
                        &SurfaceConfiguration {
                            usage: TextureUsages::RENDER_ATTACHMENT,
                            format,
                            width: size.width,
                            height: size.height,
                            present_mode: PresentMode::Mailbox,
                        },
                    );

                    resized = false;
                }

                match surface.get_current_texture() {
                    Ok(frame) => {
                        let mut encoder = device
                            .create_command_encoder(&CommandEncoderDescriptor { label: None });

                        let program = state.program();
                        let (tx, rx) = mpsc::channel();
                        stream
                            .send(kara_audio::stream::Event::RequestData(tx))
                            .unwrap();
                        let mut buffer = rx.recv().unwrap();
                        for i in 0..buffer.len() {
                            buffer.insert(0, buffer[i * 2]);
                        }

                        let (vertices, indices) = graphics::from_buffer(
                            buffer,
                            1.5,
                            [0.0, 0.01, 0.02],
                            [0.01, 0.0, 0.05],
                            [
                                window.inner_size().width as f32 * 0.001,
                                window.inner_size().height as f32 * 0.001,
                            ],
                        );

                        let vertex_buffer =
                            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Vertex Buffer"),
                                contents: bytemuck::cast_slice(&vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });
                        let index_buffer =
                            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Index Buffer"),
                                contents: bytemuck::cast_slice(&indices),
                                usage: wgpu::BufferUsages::INDEX,
                            });

                        let view = frame.texture.create_view(&TextureViewDescriptor::default());

                        {
                            // We clear the frame
                            let mut render_pass =
                                scene.clear(&view, &mut encoder, program.background_color());

                            // Draw the scene
                            scene.draw(
                                &mut render_pass,
                                &vertex_buffer,
                                &index_buffer,
                                indices.len(),
                            );
                        }

                        // And then iced on top
                        renderer.with_primitives(|backend, primitive| {
                            backend.present(
                                &device,
                                &mut staging_belt,
                                &mut encoder,
                                &view,
                                primitive,
                                &viewport,
                                &debug.overlay(),
                            );
                        });

                        // Then we submit the work
                        staging_belt.finish();
                        //                 queue.submit(Some(encoder.finish()));

                        queue.submit(std::iter::once(encoder.finish()));
                        frame.present();

                        // Update the mouse cursor
                        window.set_cursor_icon(iced_winit::conversion::mouse_interaction(
                            state.mouse_interaction(),
                        ));

                        // And recall staging buffers
                        local_pool
                            .spawner()
                            .spawn(staging_belt.recall())
                            .expect("Recall staging buffers");

                        local_pool.run_until_stalled();
                    }
                    Err(error) => match error {
                        SurfaceError::OutOfMemory => {
                            error!("Swapchain error: {}. Rendering cannot continue.", error);
                            panic!("Swapchain error: {}. Rendering cannot continue.", error)
                        }
                        _ => {
                            // Try rendering again next frame.
                            window.request_redraw();
                        }
                    },
                }
                window.request_redraw()
            }
            // Receiving feed (speech) from the user
            Event::UserEvent(val) => {
                state.queue_message(controls::Message::TextChanged(val));
            }
            _ => {}
        }
    });
}
mod controls {
    use iced_wgpu::Renderer;
    use iced_winit::{
        alignment,
        widget::{Column, Container, Text},
        Color, Length, Program,
    };

    pub struct Controls {
        background_color: Color,
        text: String,
    }

    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub enum Message {
        TextChanged(String),
    }

    impl Controls {
        pub fn new(opacity: f32) -> Self {
            Self {
                background_color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: opacity,
                },
                text: String::from("Hey there"),
            }
        }

        pub fn background_color(&self) -> Color {
            self.background_color
        }
    }

    impl Program for Controls {
        type Renderer = Renderer;

        type Message = Message;

        fn update(&mut self, message: Self::Message) -> iced_winit::Command<Self::Message> {
            match message {
                Message::TextChanged(val) => self.text = val,
            }
            iced_winit::Command::none()
        }

        fn view(&mut self) -> iced_winit::Element<'_, Self::Message, Self::Renderer> {
            Container::new(
                Column::new()
                    .align_items(iced_winit::Alignment::Center)
                    .spacing(20)
                    .padding(10)
                    .push(
                        Text::new(&self.text)
                            .color(Color::new(0.949_019_6, 0.898_039_2, 0.737_254_9, 1.0))
                            .size(28),
                    ),
            )
            .padding(100)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(alignment::Vertical::Bottom)
            .center_x()
            .into()
        }
    }
}

mod scene {
    use iced_wgpu::{wgpu, Color};

    use super::abstraction::Vertex;

    pub struct Scene {
        pipeline: wgpu::RenderPipeline,
    }

    impl Scene {
        pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Self {
            Self {
                pipeline: build_pipeline(device, texture_format),
            }
        }
        pub fn clear<'a>(
            &self,
            target: &'a wgpu::TextureView,
            encoder: &'a mut wgpu::CommandEncoder,
            background_color: Color,
        ) -> wgpu::RenderPass<'a> {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear({
                            let [r, g, b, a] = background_color.into_linear();

                            wgpu::Color {
                                r: r as f64,
                                g: g as f64,
                                b: b as f64,
                                a: a as f64,
                            }
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            })
        }

        pub fn draw<'a>(
            &'a self,
            render_pass: &mut wgpu::RenderPass<'a>,
            vertex_buffer: &'a wgpu::Buffer,
            index_buffer: &'a wgpu::Buffer,
            len: usize,
        ) {
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32); // 1.
            render_pass.draw_indexed(0..len as u32, 0, 0..1); // 2.
                                                              // render_pass.draw(0..3, 0..1);
        }
    }

    fn build_pipeline(
        device: &wgpu::Device,
        texture_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let shader_module =
            device.create_shader_module(&wgpu::include_wgsl!("../kara-assets/wgpu/shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: "vertex_main",
                buffers: &[Vertex::desc()],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: "fragment_main",
                targets: &[texture_format.into()],
            }),
            multiview: None,
        })
    }
}

mod abstraction {
    use iced_wgpu::wgpu;

    #[repr(C)]
    #[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct Vertex {
        pub position: [f32; 3],
        pub color: [f32; 3],
    }
    impl Vertex {
        pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x3,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x3,
                    },
                ],
            }
        }
    }
}

mod graphics {
    use std::f32::consts::PI;

    use super::abstraction::Vertex;

    pub fn from_buffer(
        buffer: Vec<f32>,
        width: f32,
        top_color: [f32; 3],
        bottom_color: [f32; 3],
        size: [f32; 2],
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        if buffer.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let width = width * 0.005;
        let radius: f32 = 0.3;
        let mut last_x: f32 = 0.0;
        let mut last_y: f32 = 0.0;

        for i in 0..buffer.len() - 1 {
            let mut angle: f32 = 2.0 * PI * (i + 1) as f32 / (buffer.len() - 2) as f32;
            let degree: f32 = 2.0 * PI / 360.0;
            angle += degree * 270.0; // rotate circle 270°

            let value: f32 = buffer[i];

            let x: f32 = angle.cos() * (value + radius) / size[0];
            let y: f32 = angle.sin() * (value + radius) / size[1];

            let r: f32 = (top_color[0] * value) + (bottom_color[0] * (1.0 / value));
            let g: f32 = (top_color[1] * value) + (bottom_color[1] * (1.0 / value));
            let b: f32 = (top_color[2] * value) + (bottom_color[2] * (1.0 / value));

            let color: [f32; 3] = [r, g, b];

            if i != 0 {
                let (mut vertices2, mut indices2) = draw_line(
                    [last_x, last_y],
                    [x, y],
                    width,
                    color,
                    vertices.len() as u32,
                    size,
                );
                vertices.append(&mut vertices2);
                indices.append(&mut indices2);
            }
            last_x = x;
            last_y = y;
        }
        (vertices, indices)
    }

    fn draw_line(
        point1: [f32; 2],
        point2: [f32; 2],
        width: f32,
        color: [f32; 3],
        vertex_len: u32,
        size: [f32; 2],
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let x1: f32 = point1[0];
        let x2: f32 = point2[0];
        let y1: f32 = point1[1];
        let y2: f32 = point2[1];

        let dx = x2 - x1;
        let dy = y2 - y1;
        let l = dx.hypot(dy);
        let u = dx * width * 0.5 / l / size[1];
        let v = dy * width * 0.5 / l / size[0];

        vertices.push(Vertex {
            position: [x1 + v, y1 - u, 0.0],
            color,
        });
        vertices.push(Vertex {
            position: [x1 - v, y1 + u, 0.0],
            color,
        });
        vertices.push(Vertex {
            position: [x2 - v, y2 + u, 0.0],
            color,
        });
        vertices.push(Vertex {
            position: [x2 + v, y2 - u, 0.0],
            color,
        });

        indices.push(vertex_len + 2);
        indices.push(vertex_len + 1);
        indices.push(vertex_len);
        indices.push(vertex_len + 2);
        indices.push(vertex_len);
        indices.push(vertex_len + 3);

        (vertices, indices)
    }
}
