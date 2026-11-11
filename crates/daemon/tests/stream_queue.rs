use wobblescope::stream::protocol::ServerMessage;
use wobblescope::stream::queue::{Entry, OutQueue, Push};

fn reliable(n: usize) -> Entry {
    Entry::Reliable(ServerMessage::Closed {
        chip: format!("chip{n}"),
        reason: "r".to_owned(),
    })
}

fn health(chip: &str, rate: f64) -> Entry {
    Entry::Health(
        chip.to_owned(),
        ServerMessage::Health {
            chip: chip.to_owned(),
            measured_rate_hz: rate,
            configured_rate_hz: None,
            overflows: None,
            errors: None,
            dropped_frames: 0,
            warnings: vec![],
        },
    )
}

fn drain(queue: &mut OutQueue) -> Vec<Entry> {
    std::iter::from_fn(|| queue.pop()).collect()
}

#[test]
fn a_full_queue_drops_the_oldest_frame_and_counts_it() {
    let mut queue = OutQueue::new(4);
    for i in 0..4u8 {
        assert_eq!(queue.push(Entry::Frame(vec![i])), Push::Queued);
    }
    assert_eq!(queue.push(Entry::Frame(vec![4])), Push::Queued);
    assert_eq!(queue.dropped_frames(), 1);
    let frames: Vec<Entry> = (1..5u8).map(|i| Entry::Frame(vec![i])).collect();
    assert_eq!(drain(&mut queue), frames);
}

#[test]
fn health_and_console_entries_coalesce() {
    let mut queue = OutQueue::new(8);
    assert_eq!(queue.push(health("a", 1.0)), Push::Queued);
    assert_eq!(queue.push(health("b", 5.0)), Push::Queued);
    assert_eq!(queue.push(health("a", 2.0)), Push::Coalesced);
    for lines in [["1", "2", "3"], ["4", "5", "6"]] {
        queue.push(Entry::Console(lines.map(str::to_owned).to_vec()));
    }
    queue.push(Entry::Console(vec!["7".to_owned()]));
    let entries = drain(&mut queue);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], health("a", 2.0));
    assert_eq!(entries[1], health("b", 5.0));
    let last_five = ["3", "4", "5", "6", "7"].map(str::to_owned).to_vec();
    assert_eq!(entries[2], Entry::Console(last_five));
}

#[test]
fn reliable_entries_are_never_dropped_and_overflow_is_reported() {
    let mut queue = OutQueue::new(4);
    for i in 0..4 {
        assert_eq!(queue.push(reliable(i)), Push::Queued);
    }
    assert_eq!(queue.push(reliable(4)), Push::Overflow);
    assert_eq!(queue.push(Entry::Frame(vec![0])), Push::Overflow);
    assert_eq!(drain(&mut queue), (0..4).map(reliable).collect::<Vec<_>>());
}

#[test]
fn a_frame_makes_room_by_evicting_only_frames() {
    let mut queue = OutQueue::new(4);
    for i in 0..3 {
        queue.push(reliable(i));
    }
    queue.push(Entry::Frame(vec![9]));
    assert_eq!(queue.push(reliable(3)), Push::Queued);
    assert_eq!(queue.dropped_frames(), 1);
    assert_eq!(drain(&mut queue), (0..4).map(reliable).collect::<Vec<_>>());
}
