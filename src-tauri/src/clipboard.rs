/// The Windows clipboard is exclusive, and listeners (clipboard
/// history, sync services, managers) grab it the moment it changes.
/// Retrying with backoff for up to ~2s beats surfacing an error for
/// what is almost always a sub-second collision.
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

/// Raw clipboard write. On Windows the text is marked to stay out of
/// the clipboard history (Win+V) and the cross-device cloud clipboard —
/// both would silently retain (or upload) anything copied here long
/// after the paste.
fn write_clipboard(text: String) -> Result<(), String> {
    clipboard_retry("write to", || {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let set = cb.set();
        #[cfg(windows)]
        let set = {
            use arboard::SetExtWindows;
            set.exclude_from_history().exclude_from_cloud()
        };
        set.text(text.clone()).map_err(|e| e.to_string())
    })
}

/// How long a copied secret stays on the clipboard before Envarsa
/// clears it — matches the 30s reveal auto-hide.
const CLIPBOARD_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// Bumped on every secret copy; an expiring timer only acts if it is
/// still the latest, so re-copying restarts the 30s rather than
/// inheriting the old deadline.
static CLIPBOARD_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Copy a secret: write it, then clear it after CLIPBOARD_TTL — but
/// only if no newer copy was made and the clipboard still holds exactly
/// what was copied, so nothing of anyone else's is ever clobbered.
///
/// Invariant: CLIPBOARD-CLEARS (ARCHITECTURE.md).
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
