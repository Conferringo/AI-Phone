//! One reusable X11 connection per runtime; pointer receipts query actual server state.
use crate::Result;
use image::{Rgb, RgbImage};
use std::sync::Mutex;
use x11rb::{
    connection::Connection,
    errors::ReplyError,
    protocol::{
        ErrorKind,
        xproto::{self, ConnectionExt as _, ImageFormat, ImageOrder},
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

pub trait Screen: Send + Sync {
    fn capture(&self) -> Result<RgbImage>;
    fn title(&self) -> Result<String>;
    fn window(&self) -> Result<WindowIdentity>;
    fn paint_focus(&self) -> Result<PaintFocus> {
        let window = self.window()?;
        Ok(PaintFocus {
            title: window.title,
            window_id: window.window_id,
        })
    }
    fn confirmed_desktop_reveal(&self, _previous: u32, _focused: u32) -> bool {
        false
    }
    fn move_to(&self, x: i16, y: i16) -> Result<()>;
    fn button(&self, button: u8, down: bool) -> Result<()>;
    fn pointer(&self) -> Result<(i32, i32)>;
}
#[derive(Default)]
pub struct PaintFocus {
    pub title: String,
    pub window_id: u32,
}

#[derive(Default)]
pub struct WindowIdentity {
    pub title: String,
    pub application_class: String,
    pub window_id: u32,
    pub process_id: u32,
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

    fn process_id(&self, window: u32) -> Result<u32> {
        let bytes = self.property(window, self.atom(b"_NET_WM_PID")?)?;
        Ok(bytes
            .get(..4)
            .map(|v| u32::from_ne_bytes(v.try_into().unwrap()))
            .unwrap_or_default())
    }

    fn application_class(&self, window: u32, process: u32) -> Result<String> {
        let class = wm_class(&self.property(window, xproto::AtomEnum::WM_CLASS.into())?);
        Ok(transient_class(window, process, class, |child| {
            let bytes = self
                .property(child, xproto::AtomEnum::WM_TRANSIENT_FOR.into())
                .ok()?;
            let parent = u32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
            Some((
                parent,
                self.process_id(parent).ok()?,
                wm_class(
                    &self
                        .property(parent, xproto::AtomEnum::WM_CLASS.into())
                        .ok()?,
                ),
            ))
        }))
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
            let process = display.process_id(window)?;
            Ok(WindowIdentity {
                title: display.window_title(window)?,
                application_class: display.application_class(window, process)?,
                window_id: window,
                process_id: process,
            })
        })
    }
    fn paint_focus(&self) -> Result<PaintFocus> {
        self.with(|display| {
            let window_id = display.active_window()?;
            let title = if window_id == 0 {
                String::new()
            } else {
                display.window_title(window_id)?
            };
            Ok(PaintFocus { title, window_id })
        })
    }

    fn confirmed_desktop_reveal(&self, previous: u32, focused: u32) -> bool {
        if previous == 0 || previous == focused {
            return false;
        }
        self.with(|display| {
            if focused != 0 && focused != display.root {
                let desktop = display.atom(b"_NET_WM_WINDOW_TYPE_DESKTOP")?;
                let kind = display.atom(b"_NET_WM_WINDOW_TYPE")?;
                let reply = display
                    .connection
                    .get_property(false, focused, kind, xproto::AtomEnum::ATOM, 0, 32)?
                    .reply()?;
                if !reply
                    .value32()
                    .is_some_and(|mut kinds| kinds.any(|kind| kind == desktop))
                {
                    return Ok(false);
                }
            }
            // BadWindow confirms destruction. A surviving or unmapped window may still be loading.
            Ok(matches!(
                display.connection.get_window_attributes(previous)?.reply(),
                Err(ReplyError::X11Error(error)) if error.error_kind == ErrorKind::Window
            ))
        })
        .unwrap_or(false)
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

fn transient_class(
    window: u32,
    process: u32,
    class: String,
    mut parent: impl FnMut(u32) -> Option<(u32, u32, String)>,
) -> String {
    let mut visited = vec![window];
    let mut class = class;
    if process == 0 {
        return class;
    }
    // Follow the physical transient relationship, never the first same-PID
    // document or a title. Keep the active dialog's window/PID for input checks.
    for _ in 0..8 {
        let Some((owner, owner_process, owner_class)) = parent(*visited.last().unwrap()) else {
            break;
        };
        if owner == 0 || owner_process != process || visited.contains(&owner) {
            break;
        }
        visited.push(owner);
        if !owner_class.is_empty() {
            class = owner_class;
        }
    }
    class
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
    #[test]
    fn dialog_identity_follows_its_actual_parent_not_another_window_in_the_process() {
        let parents = [
            (20, (10, 180, "libreoffice-draw".into())),
            (40, (30, 180, "libreoffice-calc".into())),
        ];
        for (dialog, expected) in [(20, "libreoffice-draw"), (40, "libreoffice-calc")] {
            assert_eq!(
                transient_class(dialog, 180, "Soffice".into(), |id| parents
                    .iter()
                    .find(|(child, _)| *child == id)
                    .map(|(_, parent)| parent.clone())),
                expected
            );
        }
    }
    #[test]
    fn missing_cross_process_and_cyclic_parents_do_not_rebind_a_dialog() {
        for process in [0, 181] {
            assert_eq!(
                transient_class(20, 180, "Soffice".into(), |_| Some((
                    10,
                    process,
                    "other-app".into()
                ))),
                "Soffice"
            );
        }
        assert_eq!(
            transient_class(20, 180, "Soffice".into(), |_| None),
            "Soffice"
        );
        assert_eq!(
            transient_class(20, 180, "Soffice".into(), |_| Some((
                20,
                180,
                "other-app".into()
            ))),
            "Soffice"
        );
        let mut calls = 0;
        transient_class(20, 180, "Soffice".into(), |id| {
            calls += 1;
            Some((id + 1, 180, String::new()))
        });
        assert_eq!(calls, 8, "malformed parent chains remain bounded");
    }
}
