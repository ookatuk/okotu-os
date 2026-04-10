extern crate alloc;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::hint::spin_loop;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use crate::{deb, log_last, result};
use core::time::Duration;
use spin::Mutex;
use spin::rwlock::RwLock;
use x86_64::instructions::interrupts::enable_and_hlt;
use x86_64::structures::idt::InterruptStackFrame;
use crate::apic_helper::send_eoi;
use crate::interrupt;
use crate::result::{Error, ErrorType};
use crate::thread_local::read_gs;
use crate::timer::Timer;
use crate::timer::tsc::TSC;
use crate::util::debug::with_interr;

pub static ASYNC_LIST: (RwLock<Vec<Executor>>, AtomicUsize) = (
    RwLock::new(Vec::new()),
    AtomicUsize::new(0)
);

pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

use core::marker::PhantomData;

pub struct Pending<T> {
    _phantom: PhantomData<T>,
}

impl<T> Future for Pending<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

pub fn pending<T>() -> Pending<T> {
    Pending { _phantom: PhantomData }
}

#[derive(Default)]
pub struct CoreExecutor {
    pub task_queue: Arc<Mutex<VecDeque<Arc<Task>>>>,
    pub tickets: Arc<Mutex<BTreeMap<Duration, Vec<Waker>>>>,
}

pub struct TimerFuture {
    deadline: Duration,
}

impl TimerFuture {
    pub fn new(dur: Duration) -> Self {
        Self { deadline: TSC.get_time() + dur }
    }
}

impl Future for TimerFuture {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if TSC.get_time() >= self.deadline {
            Poll::Ready(())
        } else {
            let core = read_gs().expect("GS not initialized");
            with_interr(|| {
                core.executor.tickets.lock().entry(self.deadline)
                    .or_insert_with(Vec::new)
                    .push(cx.waker().clone());
                Poll::Pending
            })
        }
    }
}

pub struct JoinHandle<T> {
    inner: Arc<JoinInner<T>>,
}

struct JoinInner<T> {
    data: Mutex<Option<T>>,
    waker: Mutex<Option<Waker>>,
}

impl<T> Future for JoinHandle<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut data = self.inner.data.lock();
        if let Some(res) = data.take() {
            Poll::Ready(res)
        } else {
            *self.inner.waker.lock() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

struct Task {
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

#[derive(Clone)]
pub struct Executor {
    inner: Arc<CoreExecutor>,
    pub online: Arc<AtomicBool>,
    pub noise: Arc<AtomicBool>,
    pub lapic_id: u32,
}

impl Executor {
    extern "x86-interrupt" fn dummy_interrupt(_: InterruptStackFrame) {
        unsafe{send_eoi()};
    }

    pub fn new() -> result::Result<Self> {
        if interrupt::api::add(
            65,
            Self::dummy_interrupt,
            false
        ).is_err() {
            return Error::new(
                ErrorType::AlreadyInitialized,
                Some("executor already initialized"),
            ).raise()
        }

        let me = Self {
            inner: Arc::new(CoreExecutor {
                task_queue: Arc::new(Mutex::new(VecDeque::new())),
                tickets: Arc::new(Mutex::new(BTreeMap::new())),
            }),
            online: Arc::new(AtomicBool::new(true)),
            noise: Arc::new(AtomicBool::new(false)),
            lapic_id: crate::cpu::utils::who_am_i().unwrap(),
        };

        with_interr(|| {
            ASYNC_LIST.0.write().push(me.clone());
        });

        Ok(me)
    }

    pub fn get_core_inner(&self) -> Arc<CoreExecutor> {
        Arc::clone(&self.inner)
    }

    pub fn spawn_global<F>(future: F) -> bool
    where F: Future<Output = ()> + Send + 'static
    {
        let (target_lapic_id, needs_ipi) = {
            let executors = ASYNC_LIST.0.read();
            if executors.is_empty() { return false; }

            let idx = ASYNC_LIST.1.fetch_add(1, Ordering::SeqCst) % executors.len();
            let executor = &executors[idx];

            executor.spawn(future);

            let already = executor.noise.swap(true, Ordering::SeqCst);
            let is_offline = !executor.online.load(Ordering::Relaxed);

            let needs_ipi = !already || is_offline;

            (executor.lapic_id, needs_ipi)
        };

        if needs_ipi {
            unsafe { crate::apic_helper::send_fixed_ipi(target_lapic_id, 65) };
        }

        true
    }

    pub fn spawn_selected_async_index<F>(index: usize, future: F) -> bool
    where F: Future<Output = ()> + Send + 'static
    {
        let (target_lapic_id, needs_ipi) = {
            let executors = ASYNC_LIST.0.read();
            if executors.len() <= index { return false; }

            let executor = &executors[index];

            executor.spawn(future);

            let already_notified = executor.noise.swap(true, Ordering::SeqCst);
            let is_offline = !executor.online.load(Ordering::Relaxed);

            let needs_ipi = !already_notified || is_offline;

            (executor.lapic_id, needs_ipi)
        };

        if needs_ipi {
            unsafe { crate::apic_helper::send_fixed_ipi(target_lapic_id, 65) };
        }

        true
    }

    pub fn spawn_selected_lapic_id<F>(id: u32, future: F) -> bool
    where F: Future<Output = ()> + Send + 'static
    {
        let (target_lapic_id, needs_ipi) = {
            let executors = ASYNC_LIST.0.read();
            let executor = executors.iter().find(|x| {
                x.lapic_id == id
            });
            if executor.is_none() { return false; }
            let executor = executor.unwrap();


            executor.spawn(future);

            let already_notified = executor.noise.swap(true, Ordering::SeqCst);
            let is_offline = !executor.online.load(Ordering::Relaxed);

            let needs_ipi = !already_notified || is_offline;

            (executor.lapic_id, needs_ipi)
        };

        if needs_ipi {
            unsafe { crate::apic_helper::send_fixed_ipi(target_lapic_id, 65) };
        }

        true
    }

    pub fn spawn<T, F>(&self, future: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let inner = Arc::new(JoinInner {
            data: Mutex::new(None),
            waker: Mutex::new(None),
        });

        let inner_clone = Arc::clone(&inner);
        let wrapped_future = async move {
            let res = future.await;
            *inner_clone.data.lock() = Some(res);
            if let Some(waker) = inner_clone.waker.lock().take() {
                waker.wake();
            }
        };

        let task = Arc::new(Task {
            future: Mutex::new(Box::pin(wrapped_future)),
        });

        self.inner.task_queue.lock().push_back(task);

        JoinHandle { inner }
    }

    fn check_timers(&self) {
        let mut tickets = self.inner.tickets.lock();
        let now = TSC.get_time();
        while let Some(entry) = tickets.first_entry() {
            if *entry.key() <= now {
                for waker in entry.remove() {
                    waker.wake();
                }
            } else {
                break;
            }
        }
    }

    pub fn run(&self) -> ! {
        loop {
            self.check_timers();

            if let Some(task) = self.inner.task_queue.lock().pop_front() {
                if self.noise.load(Ordering::Relaxed) {
                    self.noise.store(false, Ordering::SeqCst);
                }

                let waker = unsafe { self.create_waker(Arc::clone(&task)) };
                let mut context = Context::from_waker(&waker);
                let mut future = task.future.lock();
                let _ = future.as_mut().poll(&mut context);
            } else {
                with_interr(|| {
                    let tickets_empty = self.inner.tickets.lock().is_empty();
                    let queue_empty = self.inner.task_queue.lock().is_empty();

                    if tickets_empty && queue_empty {
                        if self.noise.swap(false, Ordering::SeqCst) {
                            return;
                        }

                        self.online.store(false, Ordering::SeqCst);
                        enable_and_hlt();
                        self.online.store(true, Ordering::SeqCst);
                    }
                });
            }
        }
    }

    unsafe fn create_waker(&self, task: Arc<Task>) -> Waker {
        struct WakerData {
            task: Arc<Task>,
            queue: Arc<Mutex<VecDeque<Arc<Task>>>>,
        }
        let data = Arc::into_raw(Arc::new(WakerData {
            task,
            queue: Arc::clone(&self.inner.task_queue),
        })) as *const ();

        unsafe fn clone(ptr: *const ()) -> RawWaker {
            let data = unsafe { Arc::from_raw(ptr as *const WakerData) };
            let cloned = Arc::clone(&data);
            let _ = Arc::into_raw(data);
            RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
        }
        unsafe fn wake(ptr: *const ()) {
            let data = unsafe { Arc::from_raw(ptr as *const WakerData) };
            data.queue.lock().push_back(Arc::clone(&data.task));
        }
        unsafe fn wake_by_ref(ptr: *const ()) {
            let data = unsafe { Arc::from_raw(ptr as *const WakerData) };
            data.queue.lock().push_back(Arc::clone(&data.task));
            let _ = Arc::into_raw(data);
        }

        unsafe fn drop(ptr: *const ()) { let _ = unsafe { Arc::from_raw(ptr as *const WakerData) }; }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
        unsafe{Waker::from_raw(RawWaker::new(data, &VTABLE))}
    }
}