/// The Windows clipboard is exclusive, and listeners (clipboard
/// history, sync services, managers) grab it the moment it changes.
/// Retrying with backoff for up to ~2s beats surfacing an error for
/// what is almost always a sub-second collision.
#[cfg(windows)]
fn clipboard_retry<T>(what: &str, mut op: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    let mut last = String::new();
    for attempt in 0..8 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(60 * attempt));
        }
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
    }
    Err(format!("could not {what} the clipboard: {last}"))
}

/// Raw clipboard write. The text is marked to stay out of the clipboard
/// history (Win+V) and the cross-device cloud clipboard — both would
/// silently retain (or upload) anything copied here long after the
/// paste.
#[cfg(windows)]
fn write_clipboard(text: String) -> Result<(), String> {
    clipboard_retry("write to", || {
        use arboard::SetExtWindows;
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let set = cb.set().exclude_from_history().exclude_from_cloud();
        set.text(text.clone()).map_err(|e| e.to_string())
    })
}

/// How long a copied secret stays on the clipboard before Envarsa
/// clears it — matches the 30s reveal auto-hide.
const CLIPBOARD_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// Bumped on every secret copy; an expiring timer only acts if it is
/// still the latest, so re-copying restarts the 30s rather than
/// inheriting the old deadline.
#[cfg(windows)]
static CLIPBOARD_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Copy a secret: write it, then clear it after CLIPBOARD_TTL — but
/// only if no newer copy was made and the clipboard still holds exactly
/// what was copied, so nothing of anyone else's is ever clobbered.
///
/// Invariant: CLIPBOARD-CLEARS (ARCHITECTURE.md).
#[cfg(windows)]
pub(crate) fn copy_secret_to_clipboard(text: String) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    write_clipboard(text.clone())?;
    let generation = CLIPBOARD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(CLIPBOARD_TTL);
        if CLIPBOARD_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Ok(mut cb) = arboard::Clipboard::new() else {
            return;
        };
        if cb.get_text().map(|t| t == text).unwrap_or(false) {
            let _ = cb.clear();
        }
    });
    Ok(())
}

/// Copy a secret through GTK's own clipboard, which works on every Linux
/// desktop, inside the Flatpak too. GTK lives on the main thread, so the
/// copy runs there: at once when called from it, else queued to it.
///
/// Invariant: CLIPBOARD-CLEARS (ARCHITECTURE.md).
#[cfg(target_os = "linux")]
pub(crate) fn copy_secret_to_clipboard(text: String) -> Result<(), String> {
    let (done, result) = std::sync::mpsc::channel();
    gtk::glib::MainContext::default().invoke(move || {
        let _ = done.send(serve_on_clipboard(text));
    });
    result
        .recv()
        .unwrap_or_else(|_| Err("could not write to the clipboard".into()))
}

/// Offer `text` on the clipboard. Envarsa keeps it and hands it out on
/// each paste, for CLIPBOARD_TTL; after that a paste gets nothing, even
/// when Envarsa can't clear the clipboard because it isn't focused. When
/// the time is up it clears the clipboard, but only if it still owns it.
/// A newer copy, or another app's, takes ownership, and GTK then drops
/// the closure that holds the value.
#[cfg(target_os = "linux")]
fn serve_on_clipboard(text: String) -> Result<(), String> {
    use gtk::{gdk, glib, TargetEntry, TargetFlags};
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Instant;

    /// Lives in the closure GTK holds, so its drop means the clipboard
    /// belongs to someone else, or was cleared.
    struct Owned(Rc<Cell<bool>>);
    impl Drop for Owned {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }

    let owned = Rc::new(Cell::new(true));
    let guard = Owned(owned.clone());
    let until = Instant::now() + CLIPBOARD_TTL;
    let targets = [
        "UTF8_STRING",
        "text/plain;charset=utf-8",
        "text/plain",
        "STRING",
        "TEXT",
    ]
    .map(|t| TargetEntry::new(t, TargetFlags::empty(), 0));
    let clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);
    let offered = clipboard.set_with_data(&targets, move |_, data, _| {
        let _owned = &guard;
        if Instant::now() < until {
            data.set_text(&text);
        }
    });
    if !offered {
        return Err("could not write to the clipboard".into());
    }
    glib::timeout_add_local_once(CLIPBOARD_TTL, move || {
        if owned.get() {
            clipboard.clear();
        }
    });
    Ok(())
}
