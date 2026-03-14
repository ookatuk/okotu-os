use crate::util::result;
use alloc::{boxed::Box, vec::Vec};
use spin::{Once, RwLock};
use x86_64::{
    instructions::interrupts::without_interrupts,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame},
};

static IDT: Once<&'static InterruptDescriptorTable> = Once::new();
static HELPER_CREATED: Once = Once::new();

pub static TIMER_INTERRUPT_LIST: RwLock<Vec<(&fn(), u64, u64)>> = RwLock::new(Vec::new());

extern "x86-interrupt" fn timer_interrupt(_stack: InterruptStackFrame) {
    let mut to_run = Vec::new();

    without_interrupts(|| {
        if let Some(mut lock) = TIMER_INTERRUPT_LIST.try_write() {
            // retainを使いつつ、条件に合うもの（実行するもの）を一旦抽出
            lock.retain(|(f, s, s2)| {
                let func_addr = ((*f) as *const fn()).addr() as u64;
                if func_addr == *s && *s == *s2 {
                    to_run.push(*f); // 実行予定リストに入れる
                    true
                } else {
                    false // 無効なものは削除
                }
            });
        }
    });
    for f in to_run {
        f();
    }

    unsafe {
        crate::util::lapic::LapicOffset::Eoi.write(0).unwrap();
    }
}

pub struct InterruptHelper;

impl InterruptHelper {
    pub fn init() -> result::Result<Self> {
        if HELPER_CREATED.is_completed() {
            return result::Error::new(
                result::ErrorType::AlreadyUsed,
                Some("interrupt helper already created."),
            )
            .raise();
        }
        HELPER_CREATED.call_once(|| unsafe {
            without_interrupts(|| {
                let idt = Box::leak(Box::new(InterruptDescriptorTable::new()));
                idt[32].set_handler_fn(timer_interrupt);

                idt.load();

                IDT.call_once(|| idt);
            });
        });

        Ok(Self)
    }
}
