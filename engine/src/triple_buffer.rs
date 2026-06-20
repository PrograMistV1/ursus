use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering};

pub struct TripleBuffer<T: Send> {
    slots: [UnsafeCell<T>; 3],
    state: AtomicU8,
}

unsafe impl<T: Send> Send for TripleBuffer<T> {}
unsafe impl<T: Send> Sync for TripleBuffer<T> {}

const NEW_FRAME_BIT: u8 = 0b1000_0000;
const READY_SHIFT: u8 = 4;
const WRITE_SHIFT: u8 = 2;
const IDX_MASK: u8 = 0b11;

const INITIAL_STATE: u8 = (1 << READY_SHIFT) | (0 << WRITE_SHIFT);

impl<T: Send + Default> TripleBuffer<T> {
    pub fn new() -> Self {
        Self {
            slots: [
                UnsafeCell::new(T::default()),
                UnsafeCell::new(T::default()),
                UnsafeCell::new(T::default()),
            ],
            state: AtomicU8::new(INITIAL_STATE),
        }
    }
}

impl<T: Send> TripleBuffer<T> {
    pub fn new_with(a: T, b: T, c: T) -> Self {
        Self {
            slots: [UnsafeCell::new(a), UnsafeCell::new(b), UnsafeCell::new(c)],
            state: AtomicU8::new(INITIAL_STATE),
        }
    }

    pub fn write_slot(&self) -> &mut T {
        let state = self.state.load(Ordering::Relaxed);
        let idx = ((state >> WRITE_SHIFT) & IDX_MASK) as usize;
        unsafe { &mut *self.slots[idx].get() }
    }

    pub fn publish(&self) {
        let mut state = self.state.load(Ordering::Relaxed);
        loop {
            let write = (state >> WRITE_SHIFT) & IDX_MASK;
            let ready = (state >> READY_SHIFT) & IDX_MASK;

            let new_state = NEW_FRAME_BIT | (write << READY_SHIFT) | (ready << WRITE_SHIFT);

            match self.state.compare_exchange_weak(state, new_state, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return,
                Err(s) => state = s,
            }
        }
    }

    pub fn consume(&self, render_idx: &mut usize) -> bool {
        let mut state = self.state.load(Ordering::Acquire);

        if state & NEW_FRAME_BIT == 0 {
            return false;
        }

        loop {
            let ready = ((state >> READY_SHIFT) & IDX_MASK) as usize;
            let render = *render_idx;

            let new_state = (state & !NEW_FRAME_BIT) & !(IDX_MASK << READY_SHIFT) | ((render as u8) << READY_SHIFT);

            match self.state.compare_exchange_weak(state, new_state, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => {
                    *render_idx = ready;
                    return true;
                }
                Err(s) => {
                    state = s;
                    if state & NEW_FRAME_BIT == 0 {
                        return false;
                    }
                }
            }
        }
    }

    pub fn render_slot(&self, render_idx: usize) -> &T {
        debug_assert!(render_idx < 3, "render_idx вне диапазона");
        unsafe { &*self.slots[render_idx].get() }
    }
}

impl<T: Send + Default> Default for TripleBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}
