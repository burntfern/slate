mod decode;
mod encode;

pub use decode::{window_icon, IconPixels};
pub use encode::{app_hicon, install_window_icon};

const ICON_SMALL: usize = 0;
const ICON_BIG: usize = 1;
const ICON_SMALL2: usize = 2;
const SMTO_ABORTIFHUNG: u32 = 0x0002;
