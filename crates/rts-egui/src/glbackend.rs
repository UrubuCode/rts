//! Backend de render OpenGL (glow) — alternativa LEVE ao wgpu/DX12.
//!
//! Motivação RAM: o backend wgpu/DX12 reserva ~224 MB de heap de driver mesmo
//! para uma UI 2D. O OpenGL não tem essa reserva — uma janela glow fica na faixa
//! de dezenas de MB. Ativado por `new Window({ render: "glow" })` (bit3 do config).
//!
//! egui_glow só PINTA sobre um contexto GL já corrente; quem cria o contexto é o
//! `glutin` (Display→Config→Context→Surface a partir da janela winit). Cada janela
//! tem seu próprio contexto GL (baratos em RAM, ao contrário do Device wgpu).

use std::num::NonZeroU32;
use std::sync::Arc;

// glow vem do RE-EXPORT do egui_glow (mesmo tipo que o `Painter` espera; usar o
// crate `glow` direto deu mismatch de tipos quando o cargo não unificou).
use egui_glow::glow;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext,
    PossiblyCurrentGlContext,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// Estado de render OpenGL de UMA janela. Tudo `!Send` (contexto/surface GL +
/// `egui_glow::Painter`). O `Arc<glow::Context>` é compartilhado com o Painter.
pub struct GlowState {
    gl_context: PossiblyCurrentContext,
    gl_surface: Surface<WindowSurface>,
    painter: egui_glow::Painter,
    gl: Arc<glow::Context>,
    /// Janela transparente → clear com alpha 0 (o painel egui também fica
    /// transparente em `end_frame`), pra o SO compor o fundo.
    transparent: bool,
    /// Caminho pendente de snapshot: setado por `request_snapshot`, consumido no
    /// próximo `paint` (lê o back buffer ANTES do swap → PPM). Para teste visual
    /// headless: garante que o frame não está em branco (texto realmente pintado).
    pending_snapshot: Option<String>,
}

impl GlowState {
    /// Cria a janela (via glutin-winit, p/ casar a config GL) + contexto GL +
    /// `egui_glow::Painter`. Retorna a `Arc<Window>` e o estado de render. Difere
    /// do caminho wgpu por NÃO usar `event_loop.create_window` direto: o glutin
    /// precisa criar a janela junto da `GlConfig` escolhida.
    pub fn build(
        event_loop: &ActiveEventLoop,
        title: &str,
        width: u32,
        height: u32,
        chrome: crate::frame::WindowChrome,
    ) -> Result<(Arc<Window>, GlowState), String> {
        let window_attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(LogicalSize::new(width as f64, height as f64))
            .with_transparent(chrome.transparent)
            .with_decorations(chrome.decorations);

        // Escolhe uma GlConfig (a de mais samples; alpha 8). `with_transparency`
        // pede uma config capaz de compor alpha quando a janela é transparente.
        // DisplayBuilder cria a janela + a config juntas.
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(chrome.transparent);
        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));
        let (window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs
                    .reduce(|acc, c| if c.num_samples() > acc.num_samples() { c } else { acc })
                    .expect("nenhuma GlConfig disponível")
            })
            .map_err(|e| format!("glutin display build: {e}"))?;
        let window = window.ok_or_else(|| "glutin não devolveu uma janela".to_string())?;

        let raw_handle = window
            .window_handle()
            .map_err(|e| format!("window handle: {e}"))?
            .as_raw();
        let gl_display = gl_config.display();

        // Contexto GL (perfil default; o loader resolve GL ou GLES conforme o SO).
        let context_attrs = ContextAttributesBuilder::new().build(Some(raw_handle));
        let not_current = unsafe {
            gl_display
                .create_context(&gl_config, &context_attrs)
                .map_err(|e| format!("create_context: {e}"))?
        };

        let window = Arc::new(window);
        // Surface da janela (GlWindow trait, glutin-winit).
        let surface_attrs = window
            .build_surface_attributes(Default::default())
            .map_err(|e| format!("surface attrs: {e}"))?;
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attrs)
                .map_err(|e| format!("create_window_surface: {e}"))?
        };

        let gl_context = not_current
            .make_current(&gl_surface)
            .map_err(|e| format!("make_current: {e}"))?;

        // VSync (1 frame), best-effort — não falha se o driver recusar.
        let _ = gl_surface.set_swap_interval(
            &gl_context,
            SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        );

        // glow::Context a partir do loader do glutin.
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|s| gl_display.get_proc_address(s).cast())
        };
        let gl = Arc::new(gl);

        let painter = egui_glow::Painter::new(gl.clone(), "", None, false)
            .map_err(|e| format!("egui_glow painter: {e}"))?;

        Ok((
            window,
            GlowState {
                gl_context,
                gl_surface,
                painter,
                gl,
                transparent: chrome.transparent,
                pending_snapshot: None,
            },
        ))
    }

    /// Agenda um snapshot do PRÓXIMO frame pintado (lido antes do swap) num PPM.
    pub fn request_snapshot(&mut self, path: String) {
        self.pending_snapshot = Some(path);
    }

    /// Lê o back buffer (`glReadPixels` RGBA) e grava um PPM P6 (RGB). GL tem
    /// origem bottom-left → invertemos as linhas para a imagem sair com o topo em
    /// cima. Chamado DENTRO do `paint`, após pintar e antes do swap.
    fn write_ppm(&self, path: &str, w: u32, h: u32) {
        use glow::HasContext as _;
        let (wi, hi) = (w as i32, h as i32);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        unsafe {
            self.gl.read_pixels(
                0,
                0,
                wi,
                hi,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
        }
        // PPM P6: header + RGB (descarta alpha), linhas invertidas (GL→imagem).
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        out.reserve((w * h * 3) as usize);
        for row in (0..h).rev() {
            let base = (row * w * 4) as usize;
            for col in 0..w as usize {
                let p = base + col * 4;
                out.push(buf[p]);
                out.push(buf[p + 1]);
                out.push(buf[p + 2]);
            }
        }
        if let Err(e) = std::fs::write(path, &out) {
            eprintln!("rts-egui snapshot: falha ao gravar {path}: {e}");
        }
    }

    /// Pinta um frame já tesselado (paint_jobs + textures_delta) e troca os
    /// buffers. Sincroniza o tamanho da surface ao `inner_size()` real todo frame
    /// (espelha o `present` do wgpu) e limpa com o mesmo fundo escuro.
    pub fn paint(
        &mut self,
        window: &Window,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));

        // Esta janela é a corrente antes de pintar (multi-janela: cada uma tem seu
        // contexto). Best-effort.
        let _ = self.gl_context.make_current(&self.gl_surface);
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(w).unwrap(),
            NonZeroU32::new(h).unwrap(),
        );

        unsafe {
            use glow::HasContext as _;
            // Transparente: clear alpha 0 (SO compõe o fundo). Opaco: fundo escuro.
            if self.transparent {
                self.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            } else {
                self.gl.clear_color(0.02, 0.02, 0.03, 1.0);
            }
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.painter
            .paint_and_update_textures([w, h], pixels_per_point, &paint_jobs, &textures_delta);

        // Snapshot agendado: lê o back buffer (já pintado) ANTES do swap.
        if let Some(path) = self.pending_snapshot.take() {
            self.write_ppm(&path, w, h);
        }

        let _ = self.gl_surface.swap_buffers(&self.gl_context);
    }

    /// Reconfigura a surface após resize. O `paint` já sincroniza todo frame, mas
    /// reagir ao evento `Resized` evita 1 frame esticado.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let _ = self.gl_context.make_current(&self.gl_surface);
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );
    }
}

impl Drop for GlowState {
    fn drop(&mut self) {
        // O Painter precisa do contexto corrente para liberar os recursos GL.
        let _ = self.gl_context.make_current(&self.gl_surface);
        self.painter.destroy();
    }
}
