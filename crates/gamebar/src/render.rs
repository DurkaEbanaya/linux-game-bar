use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use std::ptr::NonNull;
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};

pub struct Gpu {
    // Drop the GPU surface before the owning LayerSurface and Wayland connection.
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    config: wgpu::SurfaceConfiguration,
}
impl Gpu {
    pub fn new(conn: &Connection, surface: &WlSurface) -> anyhow::Result<Self> {
        let display = WaylandDisplayHandle::new(
            NonNull::new(conn.backend().display_ptr().cast())
                .ok_or_else(|| anyhow::anyhow!("No Wayland display"))?,
        );
        let window = WaylandWindowHandle::new(
            NonNull::new(surface.id().as_ptr().cast())
                .ok_or_else(|| anyhow::anyhow!("No Wayland surface"))?,
        );
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(descriptor);
        // Caller retains both objects until this Gpu is dropped; hide() explicitly drops Gpu first.
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Wayland(display)),
                raw_window_handle: RawWindowHandle::Wayland(window),
            })
        }?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("Game Bar UI"),
                ..Default::default()
            }))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let alpha = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|a| *a == wgpu::CompositeAlphaMode::PreMultiplied)
            .ok_or_else(|| anyhow::anyhow!("Compositor has no premultiplied alpha surface"))?;
        let mut config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or_else(|| anyhow::anyhow!("No surface configuration"))?;
        config.format = format;
        config.alpha_mode = alpha;
        config.present_mode = wgpu::PresentMode::Fifo;
        let renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        Ok(Self {
            surface,
            device,
            queue,
            renderer,
            config,
        })
    }
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        output: egui::FullOutput,
        size: [u32; 2],
    ) -> anyhow::Result<()> {
        if self.config.width != size[0] || self.config.height != size[1] {
            self.config.width = size[0];
            self.config.height = size[1];
            self.surface.configure(&self.device, &self.config);
        }
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: output.pixels_per_point,
        };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            other => anyhow::bail!("Surface acquisition: {other:?}"),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let commands =
            self.renderer
                .update_buffers(&self.device, &self.queue, &mut encoder, &jobs, &screen);
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &attachments,
                ..Default::default()
            });
            self.renderer
                .render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        self.queue.submit(
            commands
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        self.queue.present(frame);
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        Ok(())
    }
}
