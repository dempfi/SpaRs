//! `Doc::with_entities` checks an update before copying, so a rejected update allocates far less
//! than the document. Its own test binary, because it replaces the global allocator.
use serde_json::json;
use spars::{Doc, EntityUpdate, Error, Span, TokenIndex};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The system allocator, counting the bytes it hands out.
struct Counting;
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded unchanged to the system allocator; counting has no effect on
// the returned memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATED.fetch_add(size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocated_by<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let before = ALLOCATED.load(Ordering::Relaxed);
    let result = work();
    (result, ALLOCATED.load(Ordering::Relaxed) - before)
}

#[test]
fn a_rejected_update_does_not_copy_the_document() {
    const WORDS: usize = 10_000;
    let tokens: Vec<_> = (0..WORDS)
        .map(|i| {
            json!({"start": i * 5, "end": i * 5 + 4, "idx": i * 5, "whitespace": i + 1 < WORDS,
                         "norm": "word", "lemma": "word", "entity_iob": "O", "entity_type": ""})
        })
        .collect();
    let snapshot = json!({"format_version": 2, "document": {
        "text": vec!["word"; WORDS].join(" "), "tokens": tokens, "entities": [],
        "tensor": vec![vec![0.5_f32; 96]; WORDS]}});
    let doc = Doc::from_json(&snapshot.to_string()).unwrap();
    let size = doc.estimated_heap_bytes();
    let update = |start, end| EntityUpdate {
        entities: vec![Span::new(TokenIndex(start), TokenIndex(end), "X")],
        ..EntityUpdate::default()
    };
    // A valid update copies the document, which the counter sees.
    let (copy, valid) = allocated_by(|| doc.with_entities(&update(0, 1)));
    assert!(copy.is_ok());
    assert!(valid >= size / 2, "{valid} of {size}");
    drop(copy);
    // Out of bounds, and two entities sharing a token (checked after a per-token scratch list).
    for update in [
        update(0, WORDS + 1),
        EntityUpdate {
            entities: vec![
                update(0, 2).entities.remove(0),
                update(1, 3).entities.remove(0),
            ],
            ..EntityUpdate::default()
        },
    ] {
        let (result, rejected) = allocated_by(|| doc.with_entities(&update));
        assert!(matches!(result, Err(Error::Bounds)), "{result:?}");
        assert!(rejected < size / 50, "{rejected} of {size}");
    }
}
