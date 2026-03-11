use alloc::string::{String, ToString};
use alloc::{format, vec};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use rhai::{Dynamic, Engine, EvalAltResult, FuncRegistration, Module, NativeCallContext, RhaiNativeFunc, Shared};
use spin::RwLock;
use uefi_raw::Status;
use uefi_raw::table::runtime::ResetType;
use x86_64::instructions::interrupts::without_interrupts;
use crate::{log_debug, log_error, log_last, log_trace, log_warn, ALLOC, MAIN_PTR};
use crate::cpu::utils::cpuid;
use crate::mem::thread_safe::{Gs, GsMainData};
use crate::util::logger::OsLog;

macro_rules! reg {
    ($module:expr, $list:expr, $prefix:expr, $name:expr, $func:expr) => {
        $list.push(alloc::format!("{}::{}", $prefix, $name));
        $module.set_native_fn($name, $func);
    };
}

pub struct Extension {
    pub name: String,
    pub func: Box<dyn Fn(NativeCallContext, Vec<Dynamic>) -> Result<Dynamic, Box<EvalAltResult>> + Send + Sync>,
}

trait RawShell {
    fn new(add: Vec<Extension>) -> Self;
    fn complete(&self, fragment: &str) -> Vec<&str>;
    fn get_output(&mut self) -> Vec<String>;
    fn get_engine(&self) -> &Engine;
    fn get_engine_mut(&mut self) -> &mut Engine;
}

pub struct Shell {
    pub engine: Engine,
    pub commands: Vec<String>,
    pub tmp_output: Arc<RwLock<Vec<String>>>,
}

impl RawShell for Shell {
    fn new(extensions: Vec<Extension>) -> Self {
        let mut me = Self {
            engine: Engine::new(),
            commands: vec![],
            tmp_output: Arc::new(RwLock::new(vec![])),
        };

        let tmp_out = me.tmp_output.clone();

        me.engine.on_print(move |text| {
            without_interrupts(|| {
                tmp_out.write().push(text.trim().to_string());
            });
        });

        let tmp_out = me.tmp_output.clone();
        me.engine.on_debug(move |text, src, pos| {
            let mut l = String::new();
            if let Some(s) = src {
                l.push_str(&format!("[{}] ", s));
            }

            l.push_str(&format!("{}: ", pos));

            l.push_str(text);

            without_interrupts(|| {
                tmp_out.write().push(l.trim().to_string());
            });
        });

        let mut root = Module::new();
        let mut cmds = &mut me.commands;

        // --- sys ---
        {
            let mut sys = Module::new();
            let prefix = "os::sys";

            reg!(&mut sys, cmds, prefix, "cpuid", |leaf: i64, sub: i64| {
                let res =  core::arch::x86_64::__cpuid_count(leaf as u32, sub as u32);
                let mut map = rhai::Map::new();
                map.insert("eax".into(), (res.eax as i64).into());
                map.insert("ebx".into(), (res.ebx as i64).into());
                map.insert("ecx".into(), (res.ecx as i64).into());
                map.insert("edx".into(), (res.edx as i64).into());

                Ok::<rhai::Map, Box<rhai::EvalAltResult>>(map)
            });

            root.set_sub_module("sys", Shared::new(sys));
        }
        // --- mem ---
        {
            let mut mem = Module::new();
            let prefix = "os::mem";

            {
                let mut capacity = Module::new();
                let prefix = format!("{}::capacity", prefix);

                reg!(&mut capacity, cmds, prefix, "get_pc_capacity", || {
                    Ok(MAIN_PTR.get().unwrap().memory_manager.max_addr.load(Ordering::SeqCst))
                });

                reg!(&mut mem, cmds, prefix, "get_os_capacity", || {
                    let tmp = ALLOC.os_allocator.get();
                    if let Some(x) = tmp {
                        Ok(x.have.load(Ordering::SeqCst) as i64)
                    } else {
                        Ok(-1)
                    }
                });

                reg!(&mut mem, cmds, prefix, "used", || {
                    let tmp = ALLOC.os_allocator.get();
                    if let Some(x) = tmp {
                        Ok(x.allocated.load(Ordering::SeqCst) as i64)
                    } else {
                        Ok(-1)
                    }
                });

                mem.set_sub_module("capacity", Shared::new(capacity));
            }

            root.set_sub_module("mem", Shared::new(mem));
        }

        me.engine.register_global_module(Shared::new(root));

        for ext in extensions {
            me.commands.push(ext.name.clone());

            me.engine.register_fn(ext.name, ext.func);
        }

        me
    }
    #[inline]
    fn complete(&self, fragment: &str) -> Vec<&str> {
        self.commands.iter()
            .filter(|c| c.starts_with(fragment))
            .map(|c| c.as_str())
            .collect()
    }
    #[inline]
    fn get_output(&mut self) -> Vec<String> {
        let old = self.commands.clone();
        self.commands.clear();
        old
    }
    #[inline]
    fn get_engine(&self) -> &Engine {
        &self.engine
    }
    #[inline]
    fn get_engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }
}

pub struct RootShell {
    pub normal: Shell,
}


impl RawShell for RootShell {
    fn new(extensions: Vec<Extension>) -> Self {
        let mut me = Self {
            normal: Shell::new(extensions)
        };

        let mut root = Module::new();
        let mut cmds = &mut me.normal.commands;


        // --- log ---
        {
            me.normal.engine.register_type_with_name::<Arc<OsLog>>("OsLog")
                .register_get("content", |log: &mut Arc<OsLog>| log.data.clone())
                .register_get("time", |log: &mut Arc<OsLog>| log.time)
                .register_get("thread", |log: &mut Arc<OsLog>| log.cpu_acpi_id)
                .register_get("tag", |log: &mut Arc<OsLog>| log.tag)
                .register_get("level", |log: &mut Arc<OsLog>| log.level)
                .register_get("by", |log: &mut Arc<OsLog>| log.by)
                .register_get("file", |log: &mut Arc<OsLog>| log.file)
                .register_get("column", |log: &mut Arc<OsLog>| log.column)
                .register_get("line", |log: &mut Arc<OsLog>| log.line)
            ;

            let mut log = Module::new();
            let prefix = "kernel::log";

            reg!(&mut log, cmds, prefix, "get", |id: i64| {
                let log = crate::util::logger::read_log(id as usize);
                Ok(log)
            });

            reg!(&mut log, cmds, prefix, "oldest_available_id", || {
                let info = crate::util::logger::get_log_min_id();
                Ok(info as i64)
            });

            {
                let mut send = Module::new();
                let prefix = format!("{}::send", prefix);

                reg!(&mut send, cmds, prefix, "trace", |text: String| {
                    log_trace!("shell", "output", "{}", text);
                    Ok(())
                });
                reg!(&mut send, cmds, prefix, "debug", |text: String| {
                    log_debug!("shell", "output", "{}", text);
                    Ok(())
                });
                reg!(&mut send, cmds, prefix, "warn", |text: String| {
                    log_warn!("shell", "output", "{}", text);
                    Ok(())
                });
                reg!(&mut send, cmds, prefix, "error", |text: String| {
                    log_error!("shell", "output", "{}", text);
                    Ok(())
                });
                reg!(&mut send, cmds, prefix, "last", |text: String| {
                    log_last!("shell", "output", "{}", text);
                    Ok(())
                });
                reg!(&mut send, cmds, prefix, "kernel_panic", |text: String| {
                    panic!("{}", text);
                    #[allow(unreachable_code)]
                    Ok(())
                });

                log.set_sub_module("send", Shared::new(send));
            }

            root.set_sub_module("log", Shared::new(log));
        }

        me.normal.engine.register_global_module(Shared::new(root));
        me
    }

    #[inline]
    fn complete(&self, fragment: &str) -> Vec<&str> {
        self.normal.complete(fragment)
    }
    #[inline]
    fn get_output(&mut self) -> Vec<String> {
        self.normal.get_output()
    }
    #[inline]
    fn get_engine(&self) -> &Engine {
        self.normal.get_engine()
    }
    #[inline]
    fn get_engine_mut(&mut self) -> &mut Engine {
        self.normal.get_engine_mut()
    }
}

impl Deref for RootShell {
    type Target = Shell;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.normal
    }
}

impl DerefMut for RootShell {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.normal
    }
}

pub struct KernelShell {
    pub normal: RootShell,
}

impl RawShell for KernelShell {
    fn new(extensions: Vec<Extension>) -> Self {
        let mut me = Self {
            normal: RootShell::new(extensions)
        };

        let mut root = Module::new();
        let mut cmds = &mut me.normal.normal.commands;

        // --- power ---
        {
            let mut power = Module::new();
            let prefix = "kernel::power";

            reg!(&mut power, cmds, prefix, "shutdown", || {
                uefi::runtime::reset(ResetType::SHUTDOWN, Status::SUCCESS, None);

                #[allow(unreachable_code)]
                Ok(())
            });
            reg!(&mut power, cmds, prefix, "reboot", || {
                uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);

                #[allow(unreachable_code)]
                Ok(())
            });

            root.set_sub_module("power", Shared::new(power));
        }

        // --- ring ---
        {
            let mut ring = Module::new();
            let prefix = "kernel::ring";

            reg!(&mut ring, cmds, prefix, "get", || {
                let cs: u16;
                unsafe { core::arch::asm!("mov {:x}, cs", out(reg) cs); }
                Ok((cs & 0b11) as i64)
            });

            root.set_sub_module("ring", Shared::new(ring));
        }

        // --- io ---
        {
            let mut io = Module::new();
            let prefix = "kernel::io";

            reg!(&mut io, cmds, prefix, "out8", |port: i64, data: i64| {
                unsafe { core::arch::asm!("out dx, al", in("dx") port as u16, in("al") data as u8); }
                Ok(())
            });
            reg!(&mut io, cmds, prefix, "in8", |port: i64| {
                let data: u8;
                unsafe { core::arch::asm!("in al, dx", out("al") data, in("dx") port as u16); }
                Ok(data as i64)
            });
            reg!(&mut io, cmds, prefix, "in32", |port: i64| {
                let data: u32;
                unsafe { core::arch::asm!("in eax, dx", out("eax") data, in("dx") port as u16); }
                Ok(data as i64)
            });
            reg!(&mut io, cmds, prefix, "out32", |port: i64, data: i64| {
                unsafe { core::arch::asm!("out dx, eax", in("dx") port as u16, in("eax") data as u32); }
                Ok(())
            });

            root.set_sub_module("io", Shared::new(io));
        }

        {
            me.engine.build_type::<GsMainData>();
            me.engine.build_type::<Gs>();

            let mut gs = Module::new();
            let prefix = "kernel::gs";

            reg!(&mut gs, cmds, prefix, "get_copy", || {
                Ok(crate::mem::thread_safe::get_mut().unwrap().clone())
            });

            root.set_sub_module("gs", Shared::new(gs));
        }

        me.normal.engine.register_global_module(Shared::new(root));
        me
    }

    #[inline]
    fn complete(&self, fragment: &str) -> Vec<&str> {
        self.normal.complete(fragment)
    }
    #[inline]
    fn get_output(&mut self) -> Vec<String> {
        self.normal.get_output()
    }
    #[inline]
    fn get_engine(&self) -> &Engine {
        self.normal.get_engine()
    }
    #[inline]
    fn get_engine_mut(&mut self) -> &mut Engine {
        self.normal.get_engine_mut()
    }
}

impl Deref for KernelShell {
    type Target = RootShell;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.normal
    }
}

impl DerefMut for KernelShell {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.normal
    }
}