//! Scrollback memory, measured with a counting global allocator (this test binary only): the
//! live heap after ED3 / RIS / `reset` must fall back to the empty-terminal level, and a wide
//! terminal must stay inside its byte budget.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use atlas_term::Vt;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new, Relaxed);
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn live() -> usize {
    LIVE.load(Relaxed)
}

/// The counter is process-wide: the cases must not overlap.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn scrollback_memory_is_reclaimed_and_bounded() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let line = "x".repeat(79) + "\n";
    let fill = |vt: &Vt, lines: usize| {
        for _ in 0..lines / 100 {
            vt.feed(line.repeat(100).as_bytes());
        }
    };

    // Baseline: an empty 80x24 terminal.
    let vt = Vt::new(80, 24, |_| {});
    let empty = live();

    // ED3 (`CSI 3 J`) gives the scrollback back, spare rows included.
    fill(&vt, 5_000);
    assert!(vt.history_lines() >= 4_900);
    let full = live();
    assert!(
        full > empty + 5_000 * 80 * 16,
        "history must cost real memory: {empty} -> {full}"
    );
    vt.feed(b"\x1b[3J");
    assert_eq!(vt.history_lines(), 0);
    let after_ed3 = live();
    assert!(
        after_ed3 < empty + (empty / 2).max(256 << 10),
        "ED3 kept {} bytes of scrollback (empty terminal: {empty})",
        after_ed3 - empty
    );

    // RIS (`ESC c`).
    fill(&vt, 5_000);
    assert!(live() > full / 2);
    vt.feed(b"\x1bc");
    assert_eq!(vt.history_lines(), 0);
    let after_ris = live();
    assert!(
        after_ris < empty + (empty / 2).max(256 << 10),
        "RIS kept {} bytes of scrollback",
        after_ris - empty
    );

    // `Vt::reset` (a hostd replay starts from scratch).
    fill(&vt, 5_000);
    vt.reset();
    assert!(
        live() < empty + (empty / 2).max(256 << 10),
        "reset kept scrollback"
    );

    // A smaller scrollback setting frees the surplus too.
    fill(&vt, 5_000);
    vt.set_options(100, "block", false);
    assert!(vt.history_lines() <= 100);
    assert!(
        live() < empty + 2 * (100 + 24) * 80 * 24 + (256 << 10),
        "shrinking the cap kept the rows"
    );
}

/// 100 000 lines at 500 columns is over a GB of cells uncapped: the byte budget tightens the line cap.
#[test]
fn a_wide_terminal_stays_inside_the_scrollback_byte_budget() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let vt = Vt::new(500, 24, |_| {});
    vt.set_options(100_000, "block", false);
    let row = "y".repeat(499) + "\n";
    let chunk = row.repeat(1_000);
    for _ in 0..150 {
        vt.feed(chunk.as_bytes());
    }
    let bytes = vt.history_bytes();
    assert!(bytes <= 64 << 20, "{bytes} bytes of history at 500 columns");
    assert!(
        bytes > 32 << 20,
        "the budget must still be used, got {bytes}"
    );
    assert!(vt.history_lines() < 100_000);
}

/// `terminal.scrollback_bytes` (Ghostty `scrollback-limit`, bytes): the budget is the user's, per terminal,
/// and live. A smaller one frees rows at once, a larger one lets a wide terminal keep more lines than the
/// 64 MiB default allowed, and the line cap still applies on top.
#[test]
fn the_scrollback_byte_budget_is_a_setting_and_applies_live() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let vt = Vt::new(200, 24, |_| {});
    vt.set_options(100_000, "block", false);
    let row = "z".repeat(199) + "\n";
    let chunk = row.repeat(1_000);
    let fill = |n: usize| (0..n).for_each(|_| vt.feed(chunk.as_bytes()));

    // 8 MiB: history stops at what 8 MiB of cells holds.
    vt.set_scrollback_bytes(8 << 20);
    fill(40);
    let small = vt.history_bytes();
    assert!(small <= 8 << 20, "{small} bytes over an 8 MiB budget");
    assert!(small > 4 << 20, "the budget is used, got {small}");
    let lines_small = vt.history_lines();

    // Raising it lets history grow past the old limit on the next output.
    vt.set_scrollback_bytes(32 << 20);
    fill(40);
    let grown = vt.history_bytes();
    assert!(grown > 16 << 20 && grown <= 32 << 20, "{grown}");
    assert!(vt.history_lines() > lines_small);

    // Lowering it frees the surplus now, not at the next line of output.
    vt.set_scrollback_bytes(2 << 20);
    assert!(vt.history_bytes() <= 2 << 20, "{}", vt.history_bytes());
    assert!(
        vt.history_lines() > 0,
        "a small budget still keeps some history"
    );

    // The line cap is still the tighter of the two when it is.
    vt.set_scrollback_bytes(512 << 20);
    vt.set_options(50, "block", false);
    assert!(vt.history_lines() <= 50);
}

/// A zero or absurd value must not turn scrollback off or exhaust memory: the line setting is how history is
/// turned off. The budget is clamped to 1 MiB..1 GiB.
#[test]
fn the_scrollback_byte_budget_is_clamped() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let vt = Vt::new(100, 24, |_| {});
    vt.set_options(100_000, "block", false);
    let chunk = ("w".repeat(99) + "\n").repeat(1_000);
    vt.set_scrollback_bytes(0);
    for _ in 0..30 {
        vt.feed(chunk.as_bytes());
    }
    let low = vt.history_bytes();
    assert!(low <= 1 << 20, "0 means the 1 MiB floor, got {low}");
    assert!(low > 256 << 10, "not zero history: {low}");
    assert_eq!(vt.scrollback_bytes(), 1 << 20);
    vt.set_scrollback_bytes(usize::MAX);
    assert_eq!(vt.scrollback_bytes(), 1 << 30);
}

/// A resize to a wider grid costs more bytes per line: the same budget keeps fewer lines (it did before the
/// setting existed; the user's value must be what is divided, not the default).
#[test]
fn a_user_budget_survives_a_resize() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let vt = Vt::new(80, 24, |_| {});
    vt.set_options(100_000, "block", false);
    vt.set_scrollback_bytes(4 << 20);
    let chunk = ("v".repeat(79) + "\n").repeat(1_000);
    for _ in 0..30 {
        vt.feed(chunk.as_bytes());
    }
    vt.resize(160, 24);
    assert!(vt.history_bytes() <= 4 << 20, "{}", vt.history_bytes());
    for _ in 0..30 {
        vt.feed(chunk.as_bytes());
    }
    assert!(vt.history_bytes() <= 4 << 20, "{}", vt.history_bytes());
}
