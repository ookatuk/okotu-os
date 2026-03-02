use crate::fonts::Text;
use crate::io::console::gop::Color;
use crate::util::result;
use crate::util::result::Error;
use crate::{fonts, io, log_custom, log_debug, log_info, util, BAR_HEIGHT, BAR_MARGIN, GUI_WAIT, MAIN_FONT};
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering::SeqCst;
use fontdue::Font;
use spin::mutex::Mutex;
use spin::{Once, RwLock};
use uefi::boot::TimerTrigger;
use uefi::proto::console::gop;
use uefi::{runtime, Event};
use uefi_raw::table::boot::{EventType, Tpl};
use uefi_raw::table::runtime::ResetType;
use uefi_raw::Status;
use x86_64::instructions::interrupts;

#[derive(Debug, Default)]
struct LogCache {
    pub data: String,
    pub cache: Arc<Vec<Text>>,
    pub last_level: Cow<'static, str>,
    pub last_time: u32,
}

fn get_dynamic_rgb(tick: u16) -> u32 {
    let tri = |mut x: u16| -> u8 {
        if x > 127 { x = 255 - x; }
        (x * 2) as u8
    };

    let r = tri((tick + 0) % 256);
    let g = tri((tick + 85) % 256);
    let b = tri((tick + 170) % 256);

    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn blend_color(bg: u32, fg: u32, alpha: u8) -> u32 {
    if alpha == 0 { return bg; }
    if alpha == 255 { return fg; }

    let a = alpha as u32;
    let inv_a = 255 - a;

    // 各成分を分解して計算 ( (color * alpha + bg * inv_alpha) / 255 )
    let r = ((fg >> 16 & 0xFF) * a + (bg >> 16 & 0xFF) * inv_a) / 255;
    let g = ((fg >> 8 & 0xFF) * a + (bg >> 8 & 0xFF) * inv_a) / 255;
    let b = ((fg & 0xFF) * a + (bg & 0xFF) * inv_a) / 255;

    (r << 16) | (g << 8) | b
}

#[derive(Debug, Default)]
pub struct DisplayManager {
    pub gop_data: Arc<RwLock<Option<Box<io::console::gop::GopData>>>>,
    pub graphic_rgb_data: AtomicU8,
    pub global_font: Once<Arc<RwLock<Option<Font>>>>,
    pub do_parent: Arc<RwLock<f64>>,
    pub last_log: Mutex<LogCache>,
    pub gop_uefi_event: Once<Event>
}

impl DisplayManager {
    pub fn init_gop(&self) -> result::Result<()> {
        log_info!("kernel", "gop", "initialization...");
        log_debug!("kernel", "gop", "getting GOP Protocol...");

        let gop = util::proto::open::<gop::GraphicsOutput>(None)?;

        let gop_data = io::console::gop::get_gop(gop)?;

        log_debug!("kernel", "gop", "global set GOP Protocol...");

        interrupts::without_interrupts(|| {
            let mut data = self.gop_data.write();

            *data = Some(Box::new(gop_data));
        });

        Ok(())
    }

    pub fn do_load_grap_in_now(&self) {
        let me = self;

        interrupts::without_interrupts(|| {
            let color = me.graphic_rgb_data.fetch_add(1, SeqCst);
            let n_color = get_dynamic_rgb(color as u16);

            if let Some(mut gop_lock) = me.gop_data.try_write() {
                if let Some(gop) = gop_lock.as_deref_mut() {
                    unsafe {
                        let _ = gop.clear(Color::from_rgb(n_color));
                    }

                    let max_bar_width = gop.w.get() - (BAR_MARGIN * 2);
                    let current_width = (*me.do_parent.as_ref().read() * max_bar_width as f64) as usize;
                    let bar_x = BAR_MARGIN;
                    let bar_y = gop.h.get() - 100;

                    let (did_it_bar_color, do_it_bar_color) = if me.last_log.lock().last_level == "last" {
                        me.last_log.lock().last_time += 1;
                        if me.last_log.lock().last_time as usize *GUI_WAIT == 100_000_000 {
                            log_custom!("s", "ds", "dis", "");
                            runtime::reset(ResetType::COLD, Status::LOAD_ERROR, None);
                        }
                        (
                            Color::new(1.0, 0.0, 0.0),
                            Color::new(1.0, 0.0, 0.0)
                        )
                    } else {
                        (
                            Color::new(1.0, 1.0, 1.0),
                            Color::from_rgb(0x444444)
                        )
                    };

                    unsafe {
                        // 2. プログレスバー描画
                        let _ = gop.draw_rect(bar_x, bar_y, max_bar_width, BAR_HEIGHT, do_it_bar_color);
                        let _ = gop.draw_rect(bar_x, bar_y, current_width, BAR_HEIGHT, did_it_bar_color);

                        // 3. フォント描画ロジック
                        if let Some(font) = me.global_font.get().unwrap().read().as_ref() {
                            let lock = util::logger::LOG_BUF.read();

                            // 文字列の取得。Stringを保持する必要があるため一旦作成
                            let raw_msg = match lock.iter().last() {
                                Some(log) => log.to_short_string(),
                                None => "funny info: If shown errors, it is error.".to_string(),
                            };

                            match lock.iter().last() {
                                Some(log) => {
                                    me.last_log.lock().last_level = Cow::Borrowed(log.level);
                                }
                                None => {

                                }
                            }

                            // キャッシュのチェックと更新
                            let glyphs = {
                                let mut lk = me.last_log.lock();
                                if lk.data == raw_msg {
                                    Arc::clone(&lk.cache)
                                } else {
                                    // 新しいログなのでラスタライズ実行
                                    let analyzed = fonts::analyze_text(font, MAIN_FONT, &raw_msg);
                                    let data = fonts::gets_with_obj(
                                        &analyzed,
                                        font,
                                        MAIN_FONT,
                                        &raw_msg,
                                        16.0,
                                        bar_x as i32,
                                        (bar_y - 25) as i32
                                    );
                                    let new_cache = Arc::new(data);
                                    lk.cache = Arc::clone(&new_cache);
                                    lk.data = raw_msg;
                                    new_cache
                                }
                            };

                            for g in glyphs.iter() {
                                for row in 0..g.height {
                                    for col in 0..g.width {
                                        let alpha = g.bitmap[row * g.width + col];
                                        if alpha == 0 { continue; }

                                        let blended_u32 = blend_color(n_color, 0xFFFFFF, alpha);

                                        if let Some(Ok(raw)) = Color::from_rgb(blended_u32).get(gop.format, gop.mask) {
                                            let offset = (g.start_y + row as i32) as usize * gop.stride.get()
                                                + (g.start_x + col as i32) as usize;
                                            gop.ptr.unwrap().as_ptr().add(offset).write_volatile(raw);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    pub extern "efiapi" fn do_load_grap(_event: uefi::Event, context: Option<NonNull<c_void>>) {
        let context_ptr = match context {
            Some(p) => p.as_ptr(),
            None => return,
        };

        // context を本来の型に復元
        let me = unsafe{&*(context_ptr as *const DisplayManager)};
        me.do_load_grap_in_now();
    }

    pub fn start_load_grap(&self) -> result::Result {
        let self_ptr = NonNull::new(core::ptr::addr_of!(*self) as *mut c_void);

        let event = unsafe {
            Error::try_raise(uefi::boot::create_event(
                EventType::TIMER | EventType::NOTIFY_SIGNAL,
                Tpl::CALLBACK,
                Some(Self::do_load_grap),
                self_ptr,
            ), Some("failed to create timer event."))?
        };

        Error::try_raise(uefi::boot::set_timer(
            &event,
            TimerTrigger::Periodic(GUI_WAIT as u64),
        ), Some("failed to set timer periodic event."))?;

        self.gop_uefi_event.call_once(|| {event});

        Ok(())
    }
}