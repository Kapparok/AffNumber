use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::core::Interface;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11ClassInstance, ID3D11DepthStencilState, ID3D11Device,
    ID3D11DeviceContext, ID3D11GeometryShader, ID3D11InputLayout, ID3D11PixelShader,
    ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView,
    ID3D11Texture2D, ID3D11VertexShader, D3D11_RENDER_TARGET_VIEW_DESC, D3D11_RTV_DIMENSION_TEXTURE2D,
    D3D11_TEXTURE2D_DESC, D3D11_VIEWPORT,
    D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
};
use windows::Win32::Graphics::Dxgi::IDXGISwapChain;

use crate::affinity;
use crate::api;

static PRESENT_OK: AtomicBool = AtomicBool::new(false);
static REGISTER_WARNED: AtomicBool = AtomicBool::new(false);
static DRAW_FAILS: AtomicU32 = AtomicU32::new(0);
static STATE: OnceLock<Mutex<Option<Painter>>> = OnceLock::new();
static LAST_ERR: Mutex<String> = Mutex::new(String::new());

struct Painter {
    swapchain_ptr: usize,
    context: ID3D11DeviceContext,
    renderer: egui_directx11::Renderer,
    ctx: egui::Context,
    rtv: Option<ID3D11RenderTargetView>,
    backup: BackupState,
    width: u32,
    height: u32,
}

pub fn present_ok() -> bool {
    PRESENT_OK.load(Ordering::Relaxed)
}

pub fn last_draw_error() -> Option<String> {
    LAST_ERR.lock().ok().map(|g| g.clone()).filter(|s| !s.is_empty())
}

pub fn start() {
    let _ = STATE.set(Mutex::new(None));
    ensure_present();
}

pub fn ensure_present() {
    if PRESENT_OK.load(Ordering::Relaxed) {
        return;
    }
    if api::register_present_callback(on_present) {
        PRESENT_OK.store(true, Ordering::Relaxed);
        api::log_info("AffNumber: DXGI Present overlay registered");
    } else if !REGISTER_WARNED.swap(true, Ordering::Relaxed) {
        api::log_warn("AffNumber: Present register failed — will retry");
    }
}

unsafe extern "C" fn on_present(swapchain: *mut c_void, _userdata: *mut c_void) {
    PRESENT_OK.store(true, Ordering::Relaxed);
    affinity::poll_toggle_hotkey();
    if swapchain.is_null() || !affinity::should_draw_badges() {
        return;
    }
    let Some(lock) = STATE.get() else {
        return;
    };
    let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    match draw_frame(&mut guard, swapchain) {
        Ok(()) => {
            DRAW_FAILS.store(0, Ordering::Relaxed);
            if let Ok(mut g) = LAST_ERR.lock() {
                g.clear();
            }
        }
        Err(e) => {
            *guard = None;
            let n = DRAW_FAILS.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut g) = LAST_ERR.lock() {
                *g = e.clone();
            }
            if n < 3 || n % 120 == 0 {
                api::log_warn(&format!("AffNumber present draw: {e}"));
            }
        }
    }
}

fn draw_frame(slot: &mut Option<Painter>, swapchain: *mut c_void) -> Result<(), String> {
    let sc = unsafe { IDXGISwapChain::from_raw(swapchain) };
    let sc = std::mem::ManuallyDrop::new(sc);
    let ptr = swapchain as usize;

    let (width, height, format) = unsafe {
        let bb: ID3D11Texture2D = sc.GetBuffer(0).map_err(|e| format!("GetBuffer: {e}"))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        bb.GetDesc(&mut desc);
        (desc.Width.max(1), desc.Height.max(1), desc.Format)
    };

    let need_new = slot.as_ref().is_none_or(|p| {
        p.swapchain_ptr != ptr || p.width != width || p.height != height || p.rtv.is_none()
    });

    if need_new {
        let device: ID3D11Device = unsafe { sc.GetDevice().map_err(|e| format!("GetDevice: {e}"))? };
        let context: ID3D11DeviceContext = unsafe {
            device
                .GetImmediateContext()
                .map_err(|e| format!("GetImmediateContext: {e}"))?
        };
        let renderer =
            egui_directx11::Renderer::new(&device).map_err(|e| format!("egui renderer: {e}"))?;
        let rtv = create_rtv(&sc, &device, format)?;
        let ctx = egui::Context::default();
        install_badge_font(&ctx);
        *slot = Some(Painter {
            swapchain_ptr: ptr,
            context,
            renderer,
            ctx,
            rtv: Some(rtv),
            backup: BackupState::default(),
            width,
            height,
        });
    }

    let painter = slot.as_mut().unwrap();
    let Some(rtv) = painter.rtv.as_ref() else {
        return Ok(());
    };

    let screen =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32));
    let raw = egui::RawInput {
        screen_rect: Some(screen),
        max_texture_side: Some(8192),
        predicted_dt: 1.0 / 60.0,
        ..Default::default()
    };

    let user_scale = affinity::size().max(0.8);
    let res_scale = (height as f32 / 1080.0).clamp(0.5, 4.0);
    let scale = user_scale * res_scale;
    let items = affinity::badge_items();
    let output = painter.ctx.run(raw, |ctx| {
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("aff_badges"));
        let painter = ctx.layer_painter(layer);
        let font = egui::FontId::proportional(16.0 * scale);
        let pad = egui::vec2(8.0 * res_scale, 4.0 * res_scale);
        let color = egui::Color32::from_rgb(235, 238, 245);
        let bg = egui::Color32::from_rgba_unmultiplied(32, 34, 40, 210);
        for (pos_i, label, v) in &items {
            if *v < 0 {
                continue;
            }
            let (fx, fy) = affinity::pos(*pos_i);
            let origin = egui::pos2(fx * width as f32, fy * height as f32);
            let text = format!("{label}  {v}");
            let galley = painter.layout_no_wrap(text, font.clone(), color);
            let rect = egui::Rect::from_min_size(origin, galley.size() + pad * 2.0);
            painter.rect_filled(rect, 0.0, bg);
            painter.galley(origin + pad, galley, color);
        }
    });

    let (renderer_output, _, _) = egui_directx11::split_output(output);
    painter.backup.save(&painter.context);
    let rendered = painter
        .renderer
        .render(&painter.context, rtv, &painter.ctx, renderer_output);
    painter.backup.restore(&painter.context);
    rendered.map_err(|e| format!("render: {e}"))
}

fn install_badge_font(ctx: &egui::Context) {
    const FONT: &[u8] = include_bytes!("../assets/Ubuntu-Light.ttf");
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert(
        "ubuntu".into(),
        std::sync::Arc::new(egui::FontData::from_static(FONT)),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "ubuntu".into());
    ctx.set_fonts(fonts);
}

fn create_rtv(
    sc: &IDXGISwapChain,
    device: &ID3D11Device,
    format: DXGI_FORMAT,
) -> Result<ID3D11RenderTargetView, String> {
    unsafe {
        let bb: ID3D11Texture2D = sc.GetBuffer(0).map_err(|e| format!("GetBuffer: {e}"))?;
        if let Ok(rtv) = try_rtv(device, &bb, None) {
            return Ok(rtv);
        }
        for f in [
            format,
            DXGI_FORMAT_R8G8B8A8_UNORM,
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        ] {
            let desc = D3D11_RENDER_TARGET_VIEW_DESC {
                Format: f,
                ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
                ..Default::default()
            };
            if let Ok(rtv) = try_rtv(device, &bb, Some(&desc)) {
                return Ok(rtv);
            }
        }
        Err(format!("CreateRTV failed for {format:?}"))
    }
}

unsafe fn try_rtv(
    device: &ID3D11Device,
    bb: &ID3D11Texture2D,
    desc: Option<&D3D11_RENDER_TARGET_VIEW_DESC>,
) -> Result<ID3D11RenderTargetView, ()> {
    let mut rtv = None;
    device
        .CreateRenderTargetView(bb, desc.map(|d| d as *const _), Some(&mut rtv))
        .map_err(|_| ())?;
    rtv.ok_or(())
}

#[derive(Default)]
struct BackupState {
    scissor_rects: [RECT; D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE as _],
    scissor_count: u32,
    viewports: [D3D11_VIEWPORT; D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE as _],
    viewport_count: u32,
    raster_state: Option<ID3D11RasterizerState>,
    blend_state: Option<ID3D11BlendState>,
    blend_factor: [f32; 4],
    blend_mask: u32,
    depth_stencil_state: Option<ID3D11DepthStencilState>,
    stencil_ref: u32,
    pixel_shader_resource: Option<ID3D11ShaderResourceView>,
    sampler: Option<ID3D11SamplerState>,
    vertex_shader: Option<ID3D11VertexShader>,
    vertex_shader_instances: ClassInstances,
    vertex_shader_instances_count: u32,
    geometry_shader: Option<ID3D11GeometryShader>,
    geometry_shader_instances: ClassInstances,
    geometry_shader_instances_count: u32,
    pixel_shader: Option<ID3D11PixelShader>,
    pixel_shader_instances: ClassInstances,
    pixel_shader_instances_count: u32,
    constant_buffer: Option<ID3D11Buffer>,
    primitive_topology: D3D_PRIMITIVE_TOPOLOGY,
    index_buffer: Option<ID3D11Buffer>,
    index_buffer_format: DXGI_FORMAT,
    index_buffer_offset: u32,
    vertex_buffer: Option<ID3D11Buffer>,
    vertex_buffer_strides: u32,
    vertex_buffer_offsets: u32,
    input_layout: Option<ID3D11InputLayout>,
}

impl BackupState {
    fn save(&mut self, ctx: &ID3D11DeviceContext) {
        unsafe {
            self.scissor_count = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE;
            self.viewport_count = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE;
            ctx.RSGetScissorRects(&mut self.scissor_count, Some(self.scissor_rects.as_mut_ptr()));
            ctx.RSGetViewports(&mut self.viewport_count, Some(self.viewports.as_mut_ptr()));
            self.raster_state = ctx.RSGetState().ok();

            ctx.OMGetBlendState(
                Some(&mut self.blend_state),
                Some(&mut self.blend_factor),
                Some(&mut self.blend_mask),
            );
            ctx.OMGetDepthStencilState(
                Some(&mut self.depth_stencil_state),
                Some(&mut self.stencil_ref),
            );

            let mut pixel_shader_resources = [None];
            ctx.PSGetShaderResources(0, Some(&mut pixel_shader_resources));
            self.pixel_shader_resource = pixel_shader_resources[0].take();

            let mut samplers = [None];
            ctx.PSGetSamplers(0, Some(&mut samplers));
            self.sampler = samplers[0].take();

            self.pixel_shader_instances_count = 256;
            self.vertex_shader_instances_count = 256;
            self.geometry_shader_instances_count = 256;

            ctx.PSGetShader(
                &mut self.pixel_shader,
                Some(self.pixel_shader_instances.as_mut_ptr()),
                Some(&mut self.pixel_shader_instances_count),
            );
            ctx.VSGetShader(
                &mut self.vertex_shader,
                Some(self.vertex_shader_instances.as_mut_ptr()),
                Some(&mut self.vertex_shader_instances_count),
            );
            ctx.GSGetShader(
                &mut self.geometry_shader,
                Some(self.geometry_shader_instances.as_mut_ptr()),
                Some(&mut self.geometry_shader_instances_count),
            );

            let mut constant_buffers = [None];
            ctx.VSGetConstantBuffers(0, Some(&mut constant_buffers));
            self.constant_buffer = constant_buffers[0].take();

            self.primitive_topology = ctx.IAGetPrimitiveTopology();
            ctx.IAGetIndexBuffer(
                Some(&mut self.index_buffer),
                Some(&mut self.index_buffer_format),
                Some(&mut self.index_buffer_offset),
            );
            ctx.IAGetVertexBuffers(
                0,
                1,
                Some(&mut self.vertex_buffer),
                Some(&mut self.vertex_buffer_strides),
                Some(&mut self.vertex_buffer_offsets),
            );
            self.input_layout = ctx.IAGetInputLayout().ok();
        }
    }

    fn restore(&mut self, ctx: &ID3D11DeviceContext) {
        unsafe {
            ctx.RSSetScissorRects(Some(&self.scissor_rects[..self.scissor_count as usize]));
            ctx.RSSetViewports(Some(&self.viewports[..self.viewport_count as usize]));
            if let Some(raster_state) = self.raster_state.take() {
                ctx.RSSetState(&raster_state);
            }

            if let Some(blend_state) = self.blend_state.take() {
                ctx.OMSetBlendState(&blend_state, Some(&self.blend_factor), self.blend_mask);
            }
            if let Some(depth_stencil_state) = self.depth_stencil_state.take() {
                ctx.OMSetDepthStencilState(&depth_stencil_state, self.stencil_ref);
            }

            ctx.PSSetShaderResources(0, Some(&[self.pixel_shader_resource.take()]));
            ctx.PSSetSamplers(0, Some(&[self.sampler.take()]));

            if let Some(pixel_shader) = self.pixel_shader.take() {
                ctx.PSSetShader(
                    &pixel_shader,
                    Some(
                        &self.pixel_shader_instances.0[..self.pixel_shader_instances_count as usize],
                    ),
                );
            }
            self.pixel_shader_instances.release();

            if let Some(vertex_shader) = self.vertex_shader.take() {
                ctx.VSSetShader(
                    &vertex_shader,
                    Some(
                        &self.vertex_shader_instances.0
                            [..self.vertex_shader_instances_count as usize],
                    ),
                );
            }
            self.vertex_shader_instances.release();

            if let Some(geometry_shader) = self.geometry_shader.take() {
                ctx.GSSetShader(
                    &geometry_shader,
                    Some(
                        &self.geometry_shader_instances.0
                            [..self.geometry_shader_instances_count as usize],
                    ),
                );
            }
            self.geometry_shader_instances.release();

            ctx.VSSetConstantBuffers(0, Some(&[self.constant_buffer.take()]));
            ctx.IASetPrimitiveTopology(self.primitive_topology);
            if let Some(index_buffer) = self.index_buffer.take() {
                ctx.IASetIndexBuffer(
                    &index_buffer,
                    self.index_buffer_format,
                    self.index_buffer_offset,
                );
            }
            ctx.IASetVertexBuffers(
                0,
                1,
                Some(&self.vertex_buffer.take()),
                Some(&self.vertex_buffer_strides),
                Some(&self.vertex_buffer_offsets),
            );
            if let Some(input_layout) = self.input_layout.take() {
                ctx.IASetInputLayout(&input_layout);
            }
        }
    }
}

struct ClassInstances([Option<ID3D11ClassInstance>; 256]);

impl ClassInstances {
    fn as_mut_ptr(&mut self) -> *mut Option<ID3D11ClassInstance> {
        &mut self.0[0]
    }

    fn release(&mut self) {
        self.0.iter().for_each(drop);
    }
}

impl Default for ClassInstances {
    fn default() -> Self {
        Self(std::array::from_fn(|_| None))
    }
}
