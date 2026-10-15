pub mod host_window;
pub mod tab_bar;

pub use host_window::HostWindow;

use std::sync::Arc;

use gpui::RenderImage;

/// Wrap decoded RGBA pixels as a gpui image frame. Shared by the tab-icon
/// extraction and the empty-state mark so they build images the same way.
pub fn render_image(rgba: image::RgbaImage) -> Arc<RenderImage> {
    Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]))
}
