use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const EVENT_CAPACITY: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrumEvent {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    Choke { group: u8 },
    AllNotesOff,
    Drain,
}

struct Queue {
    slots: [Slot; EVENT_CAPACITY],
    head: AtomicUsize,
    tail: AtomicUsize,
}

struct Slot {
    sequence: AtomicUsize,
    event: UnsafeCell<MaybeUninit<DrumEvent>>,
}

// Producers and the audio callback access a slot only after acquiring its
// sequence number. The event itself is published and retired with Release.
unsafe impl Sync for Queue {}

impl Queue {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|index| Slot {
                sequence: AtomicUsize::new(index),
                event: UnsafeCell::new(MaybeUninit::uninit()),
            }),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
}

#[derive(Clone)]
pub struct EventSender {
    queue: Arc<Queue>,
}

pub struct EventReceiver {
    queue: Arc<Queue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PushError;

pub fn event_queue() -> (EventSender, EventReceiver) {
    let queue = Arc::new(Queue::new());
    (
        EventSender {
            queue: Arc::clone(&queue),
        },
        EventReceiver { queue },
    )
}

impl EventSender {
    /// Non-blocking bounded multi-producer publication. A full or contended
    /// queue rejects the event instead of allocating or delaying a producer.
    pub fn push(&self, event: DrumEvent) -> Result<(), PushError> {
        for _ in 0..8 {
            let tail = self.queue.tail.load(Ordering::Relaxed);
            let slot = &self.queue.slots[tail % EVENT_CAPACITY];
            let sequence = slot.sequence.load(Ordering::Acquire);
            let difference = sequence.wrapping_sub(tail) as isize;
            if difference < 0 {
                return Err(PushError);
            }
            if difference > 0
                || self
                    .queue
                    .tail
                    .compare_exchange_weak(
                        tail,
                        tail.wrapping_add(1),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_err()
            {
                continue;
            }
            // SAFETY: this producer exclusively reserved `tail`.
            unsafe { (*slot.event.get()).write(event) };
            slot.sequence.store(tail.wrapping_add(1), Ordering::Release);
            return Ok(());
        }
        Err(PushError)
    }

    pub fn all_notes_off(&self) -> Result<(), PushError> {
        self.push(DrumEvent::AllNotesOff)
    }
}

impl EventReceiver {
    pub fn pop(&mut self) -> Option<DrumEvent> {
        let head = self.queue.head.load(Ordering::Relaxed);
        let slot = &self.queue.slots[head % EVENT_CAPACITY];
        if slot.sequence.load(Ordering::Acquire) != head.wrapping_add(1) {
            return None;
        }
        // SAFETY: acquire observed the completed write for `head`, and this is
        // the only consumer.
        let event = unsafe { (*slot.event.get()).assume_init_read() };
        slot.sequence
            .store(head.wrapping_add(EVENT_CAPACITY), Ordering::Release);
        self.queue
            .head
            .store(head.wrapping_add(1), Ordering::Release);
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_is_bounded_ordered_and_recovers_after_full() {
        let (sender, mut receiver) = event_queue();
        for note in 0..EVENT_CAPACITY {
            sender
                .push(DrumEvent::NoteOn {
                    note: (note % 128) as u8,
                    velocity: 100,
                })
                .unwrap();
        }
        assert_eq!(sender.push(DrumEvent::AllNotesOff), Err(PushError));
        for note in 0..EVENT_CAPACITY {
            assert_eq!(
                receiver.pop(),
                Some(DrumEvent::NoteOn {
                    note: (note % 128) as u8,
                    velocity: 100,
                })
            );
        }
        assert_eq!(receiver.pop(), None);
        sender.all_notes_off().unwrap();
        assert_eq!(receiver.pop(), Some(DrumEvent::AllNotesOff));
    }
}
