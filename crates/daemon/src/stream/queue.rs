//! The bounded queue between a viewer's chip tasks and its websocket.
//!
//! Producers never wait on it. When it is full, the oldest binary frame is dropped, then the
//! oldest replaceable text to admit replaceable text or reliable messages (a frame evicts only
//! frames, and is itself dropped when there is none); replaceable text also coalesces with what is already queued.
//! Messages that must arrive are never dropped, so a queue full of them reports overflow.

use std::collections::VecDeque;

use super::protocol::ServerMessage;

/// How many console lines a coalesced entry keeps.
const CONSOLE_LINES: usize = 5;

/// One thing waiting to be sent.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// An encoded binary sample frame.
    Frame(Vec<u8>),
    /// A health message for the named chip. A newer one replaces a queued one.
    Health(String, ServerMessage),
    /// Console lines. Queued console entries merge and keep the last few lines.
    Console(Vec<String>),
    /// A message that must not be lost.
    Reliable(ServerMessage),
}

/// What happened to a pushed entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push {
    /// The entry was appended.
    Queued,
    /// The entry replaced or merged into a queued one.
    Coalesced,
    /// A frame arrived with no older frame to evict, so the incoming frame was dropped and
    /// counted. This is not an overflow.
    Dropped,
    /// The queue holds its capacity of entries that can be neither dropped nor coalesced, and
    /// the entry was not queued.
    Overflow,
}

/// A first-in first-out queue of at most `capacity` entries.
#[derive(Debug)]
pub struct OutQueue {
    capacity: usize,
    entries: VecDeque<Entry>,
    dropped_frames: u64,
}

impl OutQueue {
    /// An empty queue holding at most `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
            dropped_frames: 0,
        }
    }

    /// Queues `entry` without waiting, dropping or coalescing older entries as needed.
    pub fn push(&mut self, entry: Entry) -> Push {
        if self.coalesce(&entry) {
            return Push::Coalesced;
        }
        if self.entries.len() >= self.capacity {
            let is_frame = matches!(entry, Entry::Frame(_));
            if is_frame && !self.evict_frame() {
                self.dropped_frames += 1;
                return Push::Dropped;
            }
            if !is_frame && !self.evict_frame() && !self.evict_text() {
                return Push::Overflow;
            }
        }
        self.entries.push_back(entry);
        Push::Queued
    }

    /// The oldest entry.
    pub fn pop(&mut self) -> Option<Entry> {
        self.entries.pop_front()
    }

    /// How many frames were dropped to make room.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    fn coalesce(&mut self, entry: &Entry) -> bool {
        match entry {
            Entry::Health(chip, _) => {
                let queued = self
                    .entries
                    .iter_mut()
                    .find(|e| matches!(e, Entry::Health(c, _) if c == chip));
                queued.map(|queued| *queued = entry.clone()).is_some()
            }
            Entry::Console(lines) => {
                let queued = self.entries.iter_mut().find_map(|e| {
                    if let Entry::Console(l) = e {
                        Some(l)
                    } else {
                        None
                    }
                });
                queued
                    .map(|queued| {
                        queued.extend(lines.iter().cloned());
                        let excess = queued.len().saturating_sub(CONSOLE_LINES);
                        queued.drain(..excess);
                    })
                    .is_some()
            }
            Entry::Frame(_) | Entry::Reliable(_) => false,
        }
    }

    /// Evicts the oldest frame and counts it. False when no frame is queued.
    fn evict_frame(&mut self) -> bool {
        let Some(at) = self
            .entries
            .iter()
            .position(|e| matches!(e, Entry::Frame(_)))
        else {
            return false;
        };
        self.entries.remove(at);
        self.dropped_frames += 1;
        true
    }

    /// Evicts the oldest replaceable text. False when none is queued.
    fn evict_text(&mut self) -> bool {
        let text = self
            .entries
            .iter()
            .position(|e| matches!(e, Entry::Health(..) | Entry::Console(_)));
        text.map(|at| self.entries.remove(at)).is_some()
    }
}
