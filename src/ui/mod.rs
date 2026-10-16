pub mod host_window;
pub mod tab_bar;

pub use host_window::HostWindow;

use std::sync::Arc;

use gpui::RenderImage;

pub fn render_image(rgba: image::RgbaImage) -> Arc<RenderImage> {
    Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]))
}
