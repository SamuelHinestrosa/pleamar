//! Runs the real runtime with counters for live Rust allocations, including
//! mlua's Rust-backed VM allocator. Native COM, direct C++/driver allocations
//! and allocator-reserved pages are not counted.
//! The normal pleamar executable does not use this allocator or sampling thread.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Tracked {
    bytes: AtomicUsize,
    blocks: AtomicUsize,
    peak: AtomicUsize,
}

impl Tracked {
    const fn new() -> Self {
        Self { bytes: AtomicUsize::new(0), blocks: AtomicUsize::new(0), peak: AtomicUsize::new(0) }
    }
    fn added(&self, size: usize) {
        let live = self.bytes.fetch_add(size, Relaxed).wrapping_add(size);
        self.peak.fetch_max(live, Relaxed);
    }
}

// Delegate every allocation and its original layout to System. Bookkeeping
// uses only atomics: allocating, locking or formatting here would recurse.
unsafe impl GlobalAlloc for Tracked {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() { self.added(layout.size()); self.blocks.fetch_add(1, Relaxed); }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() { self.added(layout.size()); self.blocks.fetch_add(1, Relaxed); }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.bytes.fetch_sub(layout.size(), Relaxed);
        self.blocks.fetch_sub(1, Relaxed);
        unsafe { System.dealloc(ptr, layout); }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            if size >= layout.size() { self.added(size - layout.size()); }
            else { self.bytes.fetch_sub(layout.size() - size, Relaxed); }
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: Tracked = Tracked::new();

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--compile-check") {
        eprintln!("usage: allocation-profile --compile-check SCENE ITERATIONS");
        std::process::exit(2);
    }
    compile_check(&args[1..]);
}

fn compile_check(arguments: &[String]) {
    let [path, count] = arguments else {
        eprintln!("usage: allocation-profile --compile-check SCENE ITERATIONS");
        std::process::exit(2);
    };
    let count: usize = count.parse().ok().filter(|n| (1..=1000).contains(n)).unwrap_or_else(|| {
        eprintln!("iterations must be between 1 and 1000");
        std::process::exit(2);
    });
    // Interned names intentionally live once per process. Warm those names and
    // lazy runtime state before measuring repeated compilation of identical input.
    let valid = pleamar::read_scene(path).is_ok();
    for _ in 0..2 { drop(pleamar::read_scene(path)); }
    let mut samples = Vec::with_capacity(count);
    let before = ALLOCATOR.bytes.load(Relaxed);
    for _ in 0..count {
        let result = pleamar::read_scene(path);
        assert_eq!(result.is_ok(), valid, "compilation changed without a source change");
        drop(result);
        samples.push(ALLOCATOR.bytes.load(Relaxed));
    }
    let after = ALLOCATOR.bytes.load(Relaxed);
    println!("{}", serde_json::json!({"valid": valid, "iterations": count,
        "before_bytes": before, "after_bytes": after, "samples": samples}));
    // This is requested live memory, not RSS, allocator caches or driver memory.
    // Allow a small fixed amount of lazy bookkeeping, never a per-reload budget.
    if after > before + 4096 { std::process::exit(1); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_live_blocks_across_growth_shrink_and_free() {
        let tracked = Tracked::new();
        unsafe {
            let small = Layout::from_size_align(64, 16).unwrap();
            let large = Layout::from_size_align(512, 16).unwrap();
            let ptr = tracked.alloc_zeroed(small);
            assert!(!ptr.is_null());
            assert!(std::slice::from_raw_parts(ptr, 64).iter().all(|b| *b == 0));
            ptr.write(37);
            let ptr = tracked.realloc(ptr, small, 512);
            assert!(!ptr.is_null());
            assert_eq!(ptr.read(), 37);
            assert_eq!(tracked.bytes.load(Relaxed), 512);
            assert_eq!(tracked.blocks.load(Relaxed), 1);
            let ptr = tracked.realloc(ptr, large, 64);
            assert!(!ptr.is_null());
            assert_eq!(ptr.read(), 37);
            assert_eq!(tracked.bytes.load(Relaxed), 64);
            assert_eq!(tracked.blocks.load(Relaxed), 1);
            tracked.dealloc(ptr, small);
            let ptr = tracked.alloc(small);
            assert!(!ptr.is_null());
            tracked.dealloc(ptr, small);
            assert_eq!(tracked.bytes.load(Relaxed), 0);
            assert_eq!(tracked.blocks.load(Relaxed), 0);
            assert_eq!(tracked.peak.load(Relaxed), 512);
        }
    }
}
