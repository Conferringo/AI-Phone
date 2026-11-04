//! One reusable X11 connection per runtime; pointer receipts query actual server state.
use crate::Result;
use image::{Rgb, RgbImage};
use std::sync::Mutex;
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{self, ConnectionExt as _, ImageFormat, ImageOrder},
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

pub trait Screen: Send + Sync {
    fn capture(&self) -> Result<RgbImage>;
    fn title(&self) -> Result<String>;
    fn window(&self) -> Result<WindowIdentity>;
    fn move_to(&self, x: i16, y: i16) -> Result<()>;
    fn button(&self, button: u8, down: bool) -> Result<()>;
    fn pointer(&self) -> Result<(i32, i32)>;
}
#[derive(Default)]
pub struct WindowIdentity {
    pub title: String,
    pub application_class: String,
}

pub struct NativeScreen {
    display: String,
    connection: Mutex<Option<Display>>,
}
struct Display {
    connection: RustConnection,
    root: u32,
    width: u16,
    height: u16,
    format: PixelFormat,
}

#[derive(Clone, Copy, Debug)]
pub struct PixelFormat {
    pub bits_per_pixel: u8,
    pub scanline_pad: u8,
    pub little_endian: bool,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
}

impl NativeScreen {
    pub fn new(display: String) -> Self {
        Self {
            display,
            connection: Mutex::new(None),
        }
    }

    fn with<T>(&self, operation: impl FnOnce(&Display) -> Result<T>) -> Result<T> {
        let mut state = self.connection.lock().map_err(|_| "X11连接状态不可用")?;
        if state.is_none() {
            *state = Some(Display::connect(&self.display)?);
        }
        operation(state.as_ref().ok_or("X11连接不可用")?)
    }
}

impl Display {
    fn connect(name: &str) -> Result<Self> {
        let (connection, index) = x11rb::connect(Some(name))?;
        let setup = connection.setup();
        let screen = setup.roots.get(index).ok_or("X11屏幕不存在")?;
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|depth| &depth.visuals)
            .find(|visual| visual.visual_id == screen.root_visual)
            .ok_or("X11颜色格式不可用")?;
        let layout = setup
            .pixmap_formats
            .iter()
            .find(|format| format.depth == screen.root_depth)
            .ok_or("X11像素格式不可用")?;
        let format = PixelFormat {
            bits_per_pixel: layout.bits_per_pixel,
            scanline_pad: layout.scanline_pad,
            little_endian: setup.image_byte_order == ImageOrder::LSB_FIRST,
            red: visual.red_mask,
            green: visual.green_mask,
            blue: visual.blue_mask,
        };
        let (root, width, height) = (screen.root, screen.width_in_pixels, screen.height_in_pixels);
        connection.xtest_get_version(2, 2)?.reply()?;
        Ok(Self {
            connection,
            root,
            width,
            height,
            format,
        })
    }

    fn atom(&self, name: &[u8]) -> Result<u32> {
        Ok(self.connection.intern_atom(false, name)?.reply()?.atom)
    }

    fn active_window(&self) -> Result<u32> {
        let atom = self.atom(b"_NET_ACTIVE_WINDOW")?;
        let reply = self
            .connection
            .get_property(false, self.root, atom, xproto::AtomEnum::WINDOW, 0, 1)?
            .reply()?;
        reply
            .value32()
            .and_then(|mut values| values.next())
            .ok_or_else(|| "没有活动窗口".into())
    }

    fn property(&self, window: u32, name: u32) -> Result<Vec<u8>> {
        Ok(self
            .connection
            .get_property(false, window, name, xproto::AtomEnum::ANY, 0, 4096)?
            .reply()?
            .value)
    }

    fn active_title(&self) -> Result<String> {
        let window = self.active_window()?;
        self.window_title(window)
    }
    fn window_title(&self, window: u32) -> Result<String> {
        let atom = self.atom(b"_NET_WM_NAME")?;
        let mut title = self.property(window, atom)?;
        if title.is_empty() {
            title = self.property(window, xproto::AtomEnum::WM_NAME.into())?;
        }
        Ok(String::from_utf8_lossy(&title).trim().into())
    }
}

impl Screen for NativeScreen {
    fn capture(&self) -> Result<RgbImage> {
        self.with(|display| {
            if (display.width, display.height) != (1280, 800) {
                return Err("桌面分辨率必须为1280×800".into());
            }
            let image = display
                .connection
                .get_image(
                    ImageFormat::Z_PIXMAP,
                    display.root,
                    0,
                    0,
                    display.width,
                    display.height,
                    u32::MAX,
                )?
                .reply()?;
            pixels(
                &image.data,
                u32::from(display.width),
                u32::from(display.height),
                display.format,
            )
        })
    }

    fn title(&self) -> Result<String> {
        self.with(Display::active_title)
    }
    fn window(&self) -> Result<WindowIdentity> {
        self.with(|display| {
            let window = display.active_window()?;
            Ok(WindowIdentity {
                title: display.window_title(window)?,
                application_class: wm_class(
                    &display.property(window, xproto::AtomEnum::WM_CLASS.into())?,
                ),
            })
        })
    }

    fn move_to(&self, x: i16, y: i16) -> Result<()> {
        self.with(|display| {
            display
                .connection
                .xtest_fake_input(xproto::MOTION_NOTIFY_EVENT, 0, 0, display.root, x, y, 0)?
                .check()?;
            Ok(())
        })
    }

    fn button(&self, button: u8, down: bool) -> Result<()> {
        self.with(|display| {
            let event = if down {
                xproto::BUTTON_PRESS_EVENT
            } else {
                xproto::BUTTON_RELEASE_EVENT
            };
            display
                .connection
                .xtest_fake_input(event, button, 0, display.root, 0, 0, 0)?
                .check()?;
            Ok(())
        })
    }

    fn pointer(&self) -> Result<(i32, i32)> {
        self.with(|display| {
            let result = display.connection.query_pointer(display.root)?.reply()?;
            Ok((i32::from(result.root_x), i32::from(result.root_y)))
        })
    }
}

fn component(pixel: u32, mask: u32) -> u8 {
    let maximum = mask >> mask.trailing_zeros();
    let value = (pixel & mask) >> mask.trailing_zeros();
    ((u64::from(value) * 255 + u64::from(maximum) / 2) / u64::from(maximum)) as u8
}

fn layout(width: u32, height: u32, format: PixelFormat) -> Result<(usize, usize)> {
    if ![16, 24, 32].contains(&format.bits_per_pixel) || ![8, 16, 32].contains(&format.scanline_pad)
    {
        return Err("不支持的X11像素布局".into());
    }
    if !(1..=1280).contains(&width)
        || !(1..=800).contains(&height)
        || [format.red, format.green, format.blue].contains(&0)
    {
        return Err("不支持的X11画面格式".into());
    }
    let bits = usize::try_from(width)? * usize::from(format.bits_per_pixel);
    let pad = usize::from(format.scanline_pad);
    Ok((
        bits.div_ceil(pad) * pad / 8,
        usize::from(format.bits_per_pixel) / 8,
    ))
}

pub fn pixels(data: &[u8], width: u32, height: u32, format: PixelFormat) -> Result<RgbImage> {
    let (stride, bytes) = layout(width, height, format)?;
    if data.len() != stride * usize::try_from(height)? {
        return Err("X11画面长度无效".into());
    }
    let mut image = RgbImage::new(width, height);
    for (y, row) in data.chunks_exact(stride).enumerate() {
        for (x, value) in row.chunks_exact(bytes).take(width as usize).enumerate() {
            let pixel = integer(value, format.little_endian);
            image.put_pixel(
                x as u32,
                y as u32,
                Rgb([
                    component(pixel, format.red),
                    component(pixel, format.green),
                    component(pixel, format.blue),
                ]),
            );
        }
    }
    Ok(image)
}

fn integer(bytes: &[u8], little_endian: bool) -> u32 {
    if little_endian {
        bytes.iter().enumerate().fold(0, |value, (index, byte)| {
            value | (u32::from(*byte) << (index * 8))
        })
    } else {
        bytes
            .iter()
            .fold(0, |value, byte| (value << 8) | u32::from(*byte))
    }
}

fn wm_class(bytes: &[u8]) -> String {
    bytes
        .split(|byte| *byte == 0)
        .nth(1)
        .and_then(|class| std::str::from_utf8(class).ok())
        .filter(|class| class.len() <= 160 && !class.chars().any(char::is_control))
        .unwrap_or("")
        .into()
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn class_is_the_machine_class_not_the_instance_or_window_title() {
        assert_eq!(
            wm_class(b"private-instance\0libreoffice-writer\0"),
            "libreoffice-writer"
        );
        assert_eq!(wm_class(b"private-instance"), "");
        assert_eq!(wm_class(b"private-instance\0\0"), "");
        assert_eq!(wm_class(b"instance\0broken\nclass\0"), "");
    }
}
