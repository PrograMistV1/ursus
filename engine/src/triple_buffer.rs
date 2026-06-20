use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering};

/// Lock-free тройной буфер.
///
/// Три слота: WRITE (главный поток пишет), READY (опубликованный кадр),
/// RENDER (рендер поток читает). Индексы хранятся упакованно в один `AtomicU8`:
///
/// ```text
/// bit 7   : флаг NEW_FRAME (есть опубликованный кадр, который рендер ещё не забрал)
/// bits 4-5: индекс READY слота  (0..=2)
/// bits 2-3: индекс WRITE слота  (0..=2)  -- не используется в state, хранится отдельно
/// bits 0-1: зарезервировано
/// ```
///
/// На практике упакуем проще — только два индекса + флаг:
///
/// ```text
/// bit 7   : NEW_FRAME
/// bits 4-5: READY index
/// bits 2-3: WRITE index
/// bits 0-1: не используются
/// ```
///
/// RENDER индекс хранится локально в рендер-потоке (только он его меняет).
pub struct TripleBuffer<T: Send> {
    slots: [UnsafeCell<T>; 3],
    /// Упакованное состояние: NEW_FRAME | READY(2б) | WRITE(2б) | 00
    state: AtomicU8,
}

// SAFETY: слоты защищены протоколом — WRITE слот трогает только главный поток,
// RENDER слот — только рендер-поток, READY слот — только при atomic swap.
unsafe impl<T: Send> Send for TripleBuffer<T> {}
unsafe impl<T: Send> Sync for TripleBuffer<T> {}

const NEW_FRAME_BIT: u8 = 0b1000_0000;
const READY_SHIFT: u8 = 4;
const WRITE_SHIFT: u8 = 2;
const IDX_MASK: u8 = 0b11;

/// Начальное состояние: write=0, ready=1, render=2 (render индекс вне state).
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
    /// Создать с явными значениями.
    pub fn new_with(a: T, b: T, c: T) -> Self {
        Self {
            slots: [UnsafeCell::new(a), UnsafeCell::new(b), UnsafeCell::new(c)],
            state: AtomicU8::new(INITIAL_STATE),
        }
    }

    // ── Главный поток ────────────────────────────────────────────────────────

    /// Получить мутабельную ссылку на WRITE слот.
    ///
    /// Вызывается только из главного потока. Ссылка действительна до следующего
    /// вызова [`publish`].
    ///
    /// # Safety
    /// Только один поток (главный) вызывает эту функцию.
    pub fn write_slot(&self) -> &mut T {
        let state = self.state.load(Ordering::Relaxed);
        let idx = ((state >> WRITE_SHIFT) & IDX_MASK) as usize;
        // SAFETY: write слот трогает только главный поток.
        unsafe { &mut *self.slots[idx].get() }
    }

    /// Опубликовать написанный кадр: swap WRITE ↔ READY, выставить NEW_FRAME.
    ///
    /// Вызывается только из главного потока после заполнения write слота.
    pub fn publish(&self) {
        let mut state = self.state.load(Ordering::Relaxed);
        loop {
            let write = (state >> WRITE_SHIFT) & IDX_MASK;
            let ready = (state >> READY_SHIFT) & IDX_MASK;

            // Меняем write и ready местами, выставляем NEW_FRAME.
            let new_state = NEW_FRAME_BIT
                | (write << READY_SHIFT)  // бывший write становится ready
                | (ready << WRITE_SHIFT); // бывший ready становится новым write

            match self.state.compare_exchange_weak(
                state,
                new_state,
                Ordering::Release, // видимость записи в слот
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(s) => state = s,
            }
        }
    }

    // ── Рендер-поток ─────────────────────────────────────────────────────────

    /// Попробовать забрать свежий кадр: swap READY ↔ render_idx, сбросить NEW_FRAME.
    ///
    /// Возвращает `true` если был новый кадр и `render_idx` обновлён.
    /// Вызывается только из рендер-потока.
    ///
    /// `render_idx` — индекс RENDER слота, хранится локально в рендер-потоке.
    pub fn consume(&self, render_idx: &mut usize) -> bool {
        let mut state = self.state.load(Ordering::Acquire);

        if state & NEW_FRAME_BIT == 0 {
            return false;
        }

        loop {
            let ready = ((state >> READY_SHIFT) & IDX_MASK) as usize;
            let render = *render_idx;

            // Меняем ready и render местами, сбрасываем NEW_FRAME.
            let new_state = (state & !NEW_FRAME_BIT) & !(IDX_MASK << READY_SHIFT) | ((render as u8) << READY_SHIFT);

            match self.state.compare_exchange_weak(state, new_state, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => {
                    *render_idx = ready;
                    return true;
                }
                Err(s) => {
                    state = s;
                    // Если NEW_FRAME уже сбросили (race с другим consume — невозможно,
                    // но добавим проверку на случай будущих изменений).
                    if state & NEW_FRAME_BIT == 0 {
                        return false;
                    }
                }
            }
        }
    }

    /// Получить иммутабельную ссылку на RENDER слот.
    ///
    /// # Safety
    /// Только рендер-поток вызывает эту функцию с тем `render_idx`, который
    /// он получил через [`consume`] или начальным значением `2`.
    pub fn render_slot(&self, render_idx: usize) -> &T {
        debug_assert!(render_idx < 3, "render_idx вне диапазона");
        // SAFETY: render слот читает только рендер-поток.
        unsafe { &*self.slots[render_idx].get() }
    }
}

impl<T: Send + Default> Default for TripleBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ── Тесты ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn initial_state_no_new_frame() {
        let buf = TripleBuffer::<u32>::new();
        let mut render_idx = 2;
        assert!(!buf.consume(&mut render_idx));
        assert_eq!(render_idx, 2);
    }

    #[test]
    fn write_publish_consume() {
        let buf = TripleBuffer::<u32>::new();
        let mut render_idx = 2usize;

        *buf.write_slot() = 42;
        buf.publish();

        assert!(buf.consume(&mut render_idx));
        assert_eq!(*buf.render_slot(render_idx), 42);
    }

    #[test]
    fn double_publish_consumer_gets_latest() {
        let buf = TripleBuffer::<u32>::new();
        let mut render_idx = 2usize;

        *buf.write_slot() = 1;
        buf.publish();

        *buf.write_slot() = 2;
        buf.publish();

        // Только один consume — должны получить последнее значение.
        assert!(buf.consume(&mut render_idx));
        assert_eq!(*buf.render_slot(render_idx), 2);

        // Второй consume — нечего забирать.
        assert!(!buf.consume(&mut render_idx));
    }

    #[test]
    fn slots_never_alias() {
        // Проверяем что write, ready, render — всегда разные слоты.
        let buf = TripleBuffer::<u32>::new();
        let mut render_idx = 2usize;

        for val in 0u32..16 {
            let state_before = buf.state.load(Ordering::Relaxed);
            let write = ((state_before >> WRITE_SHIFT) & IDX_MASK) as usize;
            let ready = ((state_before >> READY_SHIFT) & IDX_MASK) as usize;

            // Все три индекса должны быть разными.
            assert_ne!(write, ready);
            assert_ne!(write, render_idx);
            assert_ne!(ready, render_idx);

            *buf.write_slot() = val;
            buf.publish();
            buf.consume(&mut render_idx);
        }
    }

    #[test]
    fn threaded_smoke() {
        let buf = Arc::new(TripleBuffer::<u32>::new());
        let buf_w = Arc::clone(&buf);

        let writer = thread::spawn(move || {
            for i in 0u32..1000 {
                *buf_w.write_slot() = i;
                buf_w.publish();
                thread::yield_now();
            }
        });

        let mut render_idx = 2usize;
        let mut last = 0u32;
        for _ in 0..1000 {
            if buf.consume(&mut render_idx) {
                let v = *buf.render_slot(render_idx);
                assert!(v >= last, "значения должны расти: got {v}, last {last}");
                last = v;
            }
            thread::yield_now();
        }

        writer.join().unwrap();
    }
}
