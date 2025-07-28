use std::cmp;
use std::sync::Arc; // Atomically Reference Counted - used for the window
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;
use std::f32::consts;

use wgpu::{util::DeviceExt};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event::StartCause;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{PhysicalKey, KeyCode};
use winit::window::{Window, WindowAttributes, WindowId};

use bytemuck::{Pod, Zeroable}; // AW: Not really sure what this is - raw buffer type punning?

// 
// Vertex - structure describing a tightly packed 'C' style
// structure for each vertex in the vertex buffer
//
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 4],
    uv: [f32; 2],
}

fn vertex(pos: glam::Vec4, uv: glam::Vec2) -> Vertex {
    Vertex {
        pos: pos.to_array(),
        uv: uv.to_array(),
    }
}

//
// Transforms - structure encapsulating the matrices we pass to the shader for transforming
// and projecting the geometry
//
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Transforms {
    view_projection: glam::Mat4,
    model: glam::Mat4,
}

impl Transforms {
    fn create_mvp_matrix() -> Self {
        let aspect_ratio = 1.0;
        let proj = glam::Mat4::perspective_rh(consts::FRAC_PI_4, aspect_ratio, 1.0, 10.0);
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(1.5f32, 0.0, 3.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );

        Transforms {
            view_projection: (proj * view),
            model: glam::Mat4::IDENTITY,
        }
    }
}

//
// AppData - probably need to rename this, but it contains all the main gubbins for the actual
// program. AppState is passed as an argument to most of these functions to provide access to
// WGPU instance/adapter/device/queue, as well as configuration information about the current
// render surface. 
//
struct AppData {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: usize,

    xforms_data: Transforms,
    xforms_ubo: wgpu::Buffer,

    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,

    render_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    
    t: f32,
}

impl AppData {
    fn create_vertices() -> (Vec<Vertex>, Vec<u16>) {
        let vertex_data = [
            vertex(glam::Vec4::new(-0.866, -0.5, 0.0, 1.0), glam::Vec2::new(0.0, 0.0)),
            vertex(glam::Vec4::new( 0.866, -0.5, 0.0, 1.0), glam::Vec2::new(1.0, 0.0)),
            vertex(glam::Vec4::new( 0.000,  1.0, 0.0, 1.0), glam::Vec2::new(0.5, 1.0)),
        ];

        let index_data: &[u16] = &[
            0, 1, 2,
        ];

        (vertex_data.to_vec(), index_data.to_vec())
    }

    fn create_vertex_buffers(app: &AppContext) -> (wgpu::Buffer, wgpu::Buffer, usize) {
        // Create the vertex and index buffers
        let (vertex_data, index_data) = Self::create_vertices();

        let vertex_buf = app.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buf = app.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&index_data),
            usage: wgpu::BufferUsages::INDEX,
        });

        (vertex_buf, index_buf, index_data.len())
    }

    fn create_uniform_buffers(app: &AppContext, xforms_data: &[Transforms]) -> wgpu::Buffer {

        let uniform_buf = app.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(xforms_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        uniform_buf
    }

    fn create_layouts(app: &AppContext) -> (wgpu::BindGroupLayout, wgpu::PipelineLayout) {
        let bind_group_layout = app.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                // Input buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        // This is the size of a single element in the buffer.
                        min_binding_size: wgpu::BufferSize::new(2 * 16 * 4), // AW: Is there a sizeof in Rust?
                        has_dynamic_offset: false,
                    },
                    count: None,
                },
            ],
        });

        // The pipeline layout describes the bind groups that a pipeline expects
        let pipeline_layout = app.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        (bind_group_layout, pipeline_layout)
    }

    fn create_render_pipeline(app: &AppContext, pipeline_layout: &wgpu::PipelineLayout ) -> wgpu::RenderPipeline {
        let module = app.device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let vertex_size = size_of::<Vertex>();

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: vertex_size as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 4 * 4,
                    shader_location: 1,
                },
            ],
        }];

        // Vertex State
        let vertex_state = wgpu::VertexState{
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &vertex_buffers,
        };

        let fragment_state = wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: app.config.view_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })]
        };

        // The pipeline is the ready-to-go program state for the GPU. It contains the shader modules,
        // the interfaces (bind group layouts) and the shader entry point.
        let pipeline = app.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: vertex_state,
            cache: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(fragment_state),
            multiview: None,
        });

        pipeline

    }

    fn create_xforms_bind_group(app: &AppContext, bind_group_layout: &wgpu::BindGroupLayout, xforms_ubo: &wgpu::Buffer) -> wgpu::BindGroup {
        // The bind group contains the actual resources to bind to the pipeline.
        //
        // Even when the buffers are individually dropped, wgpu will keep the bind group and buffers
        // alive until the bind group itself is dropped.
        let bind_group = app.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: xforms_ubo.as_entire_binding(),
                },
            ],  
        });

        bind_group
    }

    fn create_render_target_view(&self, app: &AppContext, surface_texture: &wgpu::SurfaceTexture) -> wgpu::TextureView {

        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Swapchain Texture View"),
            format: Some(app.config.view_format),
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage: Some(wgpu::TextureUsages::RENDER_ATTACHMENT),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(1),
        });

        view
    }

    fn record_command_buffer(&self, app: &AppContext, rtv: &wgpu::TextureView) -> wgpu::CommandBuffer {

        // The command encoder allows us to record commands that we will later submit to the GPU.
        let mut encoder =
            app.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let rpd = wgpu::RenderPassDescriptor {
            label: Some("Triangle Render"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: rtv,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.03,
                            g: 0.01,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };

        // Scope the render pass - the encoder is borrowed while the render pass is
        // open. When render_pass goes out of scope encoder is returned.
        //
        // Alternative is to render_pass.forget_lifetime()
        {
            let mut render_pass = encoder.begin_render_pass(&rpd);

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);

            // Take full slice of vertex buffer
            render_pass.set_vertex_buffer(0, self.vertex_buf.slice(..));

            // Bind Index buffer
            render_pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);

            // Pass ranges for index count (encapsulates base_index and index_count)
            // and instance_count (base_instance, instance_count)
            render_pass.draw_indexed(0..self.index_count as u32, 0, 0..1);
        }

        // We finish the encoder, giving us a fully recorded command buffer.
        let command_buffer = encoder.finish();

        command_buffer
    }

    fn render_frame(&mut self, app: &AppContext, delta_t: f32) {

        self.t = (self.t + delta_t) % (2.0 * consts::PI);

        //println!("render_frame {0}", self.t);

        self.xforms_data.model = glam::Mat4::from_axis_angle(glam::Vec3::Y, self.t);

        app.queue.write_buffer(&self.xforms_ubo, 16 * 4, bytemuck::cast_slice(&[self.xforms_data.model]));
        
        let surface_texture = app.surface.get_current_texture()
            .expect("Failed to acquire next swap chain texture");

        let render_view = self.create_render_target_view(app, &surface_texture);

        let command_buffer = self.record_command_buffer(app, &render_view);

        app.queue.submit([command_buffer]);

        surface_texture.present();
    }

    fn init(app: &AppContext) -> Self {

        let (vb, ib, index_count) = Self::create_vertex_buffers(app);

        let xforms_data = Transforms::create_mvp_matrix();

        let xforms_ubo = Self::create_uniform_buffers(app, &[xforms_data]);

        let (bind_group_layout, pipeline_layout) = Self::create_layouts(app);
    
        let render_pipeline = Self::create_render_pipeline(app, &pipeline_layout);

        let bind_group = Self::create_xforms_bind_group(app, &bind_group_layout, &xforms_ubo);

        let appdata = AppData {
            vertex_buf: vb,
            index_buf: ib,
            index_count: index_count,
            xforms_data: xforms_data,
            xforms_ubo: xforms_ubo,
            bind_group_layout: bind_group_layout,
            pipeline_layout: pipeline_layout,
            render_pipeline: render_pipeline,
            bind_group: bind_group,
            t: 0.0,
        };

        appdata
    }

}

//
// AppState - wrapper for the main objects of WGPU, such as the instance/adapter/device/queuee.
//
// The window is passed as a creation parameter, a store of configuration information is extracted
// from the window while creating the surfaces.
//
// AppState is passed as a parameter to the AppData structure that contains the main portion of the
// application logic and data and is used for allowing the application to create it's objects and
// interface with WGPU without needing to be directly aware of windows and events.
//
struct SurfaceConfig {
    size: winit::dpi::PhysicalSize<u32>,
    surface_format: wgpu::TextureFormat,
    view_format: wgpu::TextureFormat,
}

struct AppContext {
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,

    config: SurfaceConfig,
    surface: wgpu::Surface<'static>, // AW: Need to read up more on lifetimes
}

struct AppState {
    // Window is an Option as we can't create windows until we're running the event handler
    window: Arc<Window>,

    // Wgpu top level objects
    ctx: AppContext,

    appdata: Option<AppData>,
}


impl AppState {

    fn resize(&mut self, size: PhysicalSize<u32>) {

        // Update config
        self.ctx.config.size.width = size.width.max(1).min(2048);
        self.ctx.config.size.height = size.height.max(1).min(2048);

        // Reconfigure surface
        self.configure_surface();
    }

    fn render(&mut self) {

        // as_ref() -> don't want a copy or to own, just borrow a reference
        // Unwrap - this will be valid at this point - might make appdata non-Optional.
        let appdata = self.appdata.as_mut().unwrap();

        appdata.render_frame(&self.ctx, 16.0 / 1000.0);
    }

    fn init(window: Arc<Window>) -> AppState {        
        // Create Instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        // Adapter (aka PhysicalDevice in Vulkan) - function is async so need to await
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("Failed to create adapter");

        // Print out some basic information about the adapter.
        println!("Running on Adapter: {:#?}", adapter.get_info());

        // Device/Queue
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .expect("Failed to create device");

        // Setup surfaces etc from window
        let size = window.inner_size();
        println!("Window size: {0} x {1}", size.width, size.height);

        // Create Surface
        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        println!("Supported surface formats:");
        for fmt in cap.formats.iter() {
            println!("\t\t{fmt:?}");
        }

        //
        // Preferentially choose a surface format
        //
        let (surface_format, view_format) = Self::choose_surface_and_view_format(cap.formats);

        println!("Chosen surface format: {surface_format:?}");
        println!("Chosen view format:    {view_format:?}");

        // Derive from the window/swapchain/etc
        let config = SurfaceConfig {
            size: size,
            view_format: view_format,
            surface_format: surface_format,
        };

        let mut state = AppState {
            window,
            ctx: AppContext {
                instance,
                device,
                queue,
                config,
                surface,
            },
            appdata: None,
        };

        // Further setup
        state.configure_surface();

        state.appdata = Some(AppData::init(&state.ctx));

        state
    }

    fn choose_surface_and_view_format(cap_formats: Vec<wgpu::TextureFormat>) -> (wgpu::TextureFormat, wgpu::TextureFormat) {
        // In WebGPU with canvas objects SRGB formats are not advertised as the canvas format
        // instead create the surface with the specified rgba8/bgra8 format and add SRGB onto
        // the list of view formats allowed to be created from the surface.
        let pref = [
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
        ];
        let mut idx = pref.len();

        for fmt in cap_formats.iter() {
            match pref.iter().position(|f| f == fmt) {
                Some(pref) => idx = cmp::min(idx, pref),
                None => continue,
            }
        }

        let surface_format = match idx {
            idx if idx < pref.len() => pref[idx],
            _ => cap_formats[0],
        };

        // Add SRGB (if SRGB not already specified)
        let view_format = surface_format.add_srgb_suffix();

        (surface_format, view_format)
    }

    fn configure_surface(&self) {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.ctx.config.surface_format,
            width: self.ctx.config.size.width,
            height: self.ctx.config.size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![self.ctx.config.view_format],
        };
        self.ctx.surface.configure(&self.ctx.device, &surface_config);
    }
}

struct App {
    state: Option<AppState>,
}


//
// App - top level struct that implements the event loop application handler traits
//
// When event loop is started and 'resumed' a window is opened and an AppState structure
// is created that will contain a reference to the window and the wgpu objects like the
// instance/adapter/device, and a store of configuration information from the window
//
// On a suspend/resume, or a resize the window surface will be re-created - not sure how
// to organise that in Rust yet, but feels like there's probably a neater way to make that
// work than I'd traditionally bothered with in C/C++
//
impl App {
    fn init() -> Self {
        let app = App {
            state: None,
        };

        app
    }
}

impl ApplicationHandler for App {

    // Initialisation 
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {

        match cause {
            StartCause::Init => {
                // First time initialisation - run once, and occurs before the first 'resumed' call
                //
                // Not actually sure what to do here as won't have a window yet
            }, 
            _ => return,
        }
    }

    // First call should create a window. Handling back-to-back redundant calls is advised
    // but is a little platform dependent.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        println!("Resumed");

        #[cfg(not(web_platform))]
        let window_attributes = WindowAttributes::default();
        #[cfg(web_platform)]
        let window_attributes = WindowAttributes::default()
            .with_platform_attributes(Box::new(WindowAttributesWeb::default().with_append(true)));

        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .unwrap(),
        );

        // pollster::block_on(...)?
        let state = AppState::init(window.clone());
        self.state = Some(state);
    }

    // Opposite of resumed - should assume all render surfaces are dead and
    // should be re-created at next 'resumed'. Again it is advisable to handle
    // redundant back-to-back calls
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        println!("Suspend");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        
        let state = self.state.as_mut().unwrap();

        // Might not be necessary...
        // let window = match state.window.id() {
        //     Some(window_id) => window,
        //     None => return,
        // };

        match event {
            WindowEvent::KeyboardInput { device_id, event, is_synthetic } => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Space) => {
                        state.window.request_redraw();        
                    },
                    PhysicalKey::Code(KeyCode::Escape) => {
                        event_loop.exit();        
                    },
                    _ => {},
                }
            },
            WindowEvent::CloseRequested => {
                println!("{event:?}");
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                //println!("Redraw");
                
                // Render Frame
                state.render();

                // Temporary rate limit
                sleep(Duration::from_millis(16));

                // Submit the next re-draw event
                state.window.request_redraw();
            },
            WindowEvent::Resized(size) => {
                println!("Resize -> {0} x {1}", size.width, size.height);
                state.resize(size);
            }
            _ => (),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    
    // To change the log level, set the `RUST_LOG` environment variable. See the `env_logger`
    // documentation for more information.
    env_logger::init();

    let event_loop = EventLoop::new()?;
    let mut app = App::init();
    event_loop.run_app(&mut app)?;

    Ok(())
}

// ------------------------------------------------------------------------------------------------------------------------------------


    // The command encoder allows us to record commands that we will later submit to the GPU.
    //let mut encoder =
    //    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // A compute pass is a single series of compute operations. While we are recording a compute
    // pass, we cannot record to the encoder.
    // let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
    //     label: None,
    //     timestamp_writes: None,
    // });

    // Set the pipeline that we want to use
    //compute_pass.set_pipeline(&pipeline);
    // Set the bind group that we want to use
    //compute_pass.set_bind_group(0, &bind_group, &[]);

    // Now we dispatch a series of workgroups. Each workgroup is a 3D grid of individual programs.
    //
    // We defined the workgroup size in the shader as 64x1x1. So in order to process all of our
    // inputs, we ceiling divide the number of inputs by 64. If the user passes 32 inputs, we will
    // dispatch 1 workgroups. If the user passes 65 inputs, we will dispatch 2 workgroups, etc.
    //let workgroup_count = arguments.len().div_ceil(64);
    //compute_pass.dispatch_workgroups(workgroup_count as u32, 1, 1);

    // Now we drop the compute pass, giving us access to the encoder again.
    //drop(compute_pass);

    // We add a copy operation to the encoder. This will copy the data from the output buffer on the
    // GPU to the download buffer on the CPU.
    // encoder.copy_buffer_to_buffer(
    //     &output_data_buffer,
    //     0,
    //     &download_buffer,
    //     0,
    //     output_data_buffer.size(),
    // );

    // We finish the encoder, giving us a fully recorded command buffer.
    //let command_buffer = encoder.finish();

    // At this point nothing has actually been executed on the gpu. We have recorded a series of
    // commands that we want to execute, but they haven't been sent to the gpu yet.
    //
    // Submitting to the queue sends the command buffer to the gpu. The gpu will then execute the
    // commands in the command buffer in order.
    //queue.submit([command_buffer]);

    // We now map the download buffer so we can read it. Mapping tells wgpu that we want to read/write
    // to the buffer directly by the CPU and it should not permit any more GPU operations on the buffer.
    //
    // Mapping requires that the GPU be finished using the buffer before it resolves, so mapping has a callback
    // to tell you when the mapping is complete.
    //let buffer_slice = download_buffer.slice(..);
    //buffer_slice.map_async(wgpu::MapMode::Read, |_| {
        // In this case we know exactly when the mapping will be finished,
        // so we don't need to do anything in the callback.
    //});

    // Wait for the GPU to finish working on the submitted work. This doesn't work on WebGPU, so we would need
    // to rely on the callback to know when the buffer is mapped.
    //device.poll(wgpu::PollType::Wait).unwrap();

    // We can now read the data from the buffer.
    //let data = buffer_slice.get_mapped_range();
    // Convert the data back to a slice of f32.
    //let result: &[f32] = bytemuck::cast_slice(&data);

    // Print out the result.
    //println!("Result: {:?}", result);