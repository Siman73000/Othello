#![allow(dead_code)]
// src/heap.rs
// Minimal bump allocator to enable `alloc` in this no_std kernel.
//
// This is intentionally simple: no free(), no reuse, just linear allocation.
// Enough to support Strings/Vectors for early FS + persistence features.

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicUsize, Ordering};

const HEAP_SIZE: usize = 4 * 1024 * 1024; // 4 MiB bump heap (tune as needed)

#[repr(align(16))]
struct Heap([u8; HEAP_SIZE]);

static mut HEAP: Heap = Heap([0u8; HEAP_SIZE]);
static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct BumpAlloc;

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(16);
        let size = layout.size();

        let base = HEAP.0.as_ptr() as usize;

        loop {
            let cur = NEXT.load(Ordering::Relaxed);
            let aligned = (cur + (align - 1)) & !(align - 1);
            let new = aligned.saturating_add(size);
            if new > HEAP_SIZE {
                return null_mut();
            }
            if NEXT.compare_exchange(cur, new, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                return (base + aligned) as *mut u8;
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // bump allocator: no dealloc
    }
}

#[global_allocator]
static ALLOC: BumpAlloc = BumpAlloc;

// Some older builds of `alloc` look for this name.
#[no_mangle]
pub extern "Rust" fn rust_oom(_layout: Layout) -> ! {
    panic!("allocation failed");
}