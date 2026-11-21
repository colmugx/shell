// SPDX-License-Identifier: GPL-3.0-or-later
//! P1-N04: bounded diagnostic scanout via Smithay's GBM swapchain.
//! No CPU copy, no periodic rendering. The caller owns the active seat
//! and must acknowledge each page-flip before reusing the swapchain.
use smithay::{
    backend::{
        allocator::{dmabuf::Dmabuf, gbm::{GbmAllocator, GbmBufferFlags, GbmDevice}, Fourcc},
        drm::{DrmDeviceFd, DrmSurface, GbmBufferedSurface},
        renderer::{gles::GlesRenderer, Bind, Color32F, Frame, Renderer},
    },
    utils::{Physical, Rectangle, Transform},
};
use std::error::Error;

pub type Buffered = GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, u8>;

pub fn create(surface: DrmSurface, gbm: GbmDevice<DrmDeviceFd>,
    renderer: &GlesRenderer) -> Result<Buffered, Box<dyn Error>> {
    let allocator = GbmAllocator::new(gbm,
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let formats = <GlesRenderer as Bind<Dmabuf>>::supported_formats(renderer)
        .ok_or("renderer does not advertise dma-buf render formats")?;
    Ok(GbmBufferedSurface::new(surface, allocator,
        &[Fourcc::Xrgb8888, Fourcc::Argb8888], formats.iter().copied())?)
}

pub fn queue(surface: &mut Buffered, renderer: &mut GlesRenderer,
    (width, height): (u16, u16), frame_id: u8) -> Result<(), Box<dyn Error>> {
    let (mut dmabuf, _) = surface.next_buffer()?;
    let size = (i32::from(width), i32::from(height)).into();
    let area: Rectangle<i32, Physical> = Rectangle::from_size(size);
    let mut target = renderer.bind(&mut dmabuf)?;
    let mut frame = renderer.render(&mut target, size, Transform::Normal)?;
    // Two distinguishable colors verify actual replacement of a frame.
    let color = if frame_id == 0 { Color32F::new(0.12, 0.22, 0.50, 1.0) }
        else { Color32F::new(0.05, 0.55, 0.38, 1.0) };
    frame.clear(color, &[area])?;
    let fence = frame.finish()?;
    // For this diagnostic explicitly await rendering completion. Production
    // will use asynchronous fences to avoid render-thread stalls.
    fence.wait()?;
    drop(target);
    surface.queue_buffer(None, None, frame_id)?;
    Ok(())
}
