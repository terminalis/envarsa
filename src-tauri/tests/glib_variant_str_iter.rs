#![cfg(target_os = "linux")]

use glib::{prelude::*, Variant};

// Run with --release: RUSTSEC-2024-0429 can leave the C out-pointer NULL
// under optimization. Exercise each entry point into VariantStrIter::impl_get.
#[test]
fn string_iteration_preserves_values_and_boundaries() {
    let values = ["", "alpha", "Grüße", "日本語", "omega"];
    let variant = Variant::array_from_iter::<String>(values.iter().map(|s| s.to_variant()));
    let iter = || variant.array_iter_str().unwrap();

    assert_eq!(iter().collect::<Vec<_>>(), values);
    assert_eq!(
        iter().rev().collect::<Vec<_>>(),
        values.into_iter().rev().collect::<Vec<_>>()
    );
    assert_eq!(iter().nth(2), Some("Grüße"));
    assert_eq!(iter().nth_back(1), Some("日本語"));
    assert_eq!(iter().last(), Some("omega"));

    let mut mixed = iter();
    assert_eq!(mixed.next(), Some(""));
    assert_eq!(mixed.next_back(), Some("omega"));
    assert_eq!(mixed.len(), 3);
    assert_eq!(mixed.nth(1), Some("Grüße"));
    assert_eq!(mixed.next_back(), Some("日本語"));
    assert_eq!(mixed.len(), 0);
    assert_eq!(mixed.next(), None);
    assert_eq!(mixed.next_back(), None);
    assert_eq!(mixed.last(), None);

    assert_eq!(iter().nth(usize::MAX), None);
    assert_eq!(iter().nth_back(usize::MAX), None);
}

#[test]
fn empty_string_array_is_exhausted() {
    let variant = Variant::array_from_iter::<String>(std::iter::empty::<Variant>());
    let mut iter = variant.array_iter_str().unwrap();
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next_back(), None);
    assert_eq!(iter.nth(0), None);
    assert_eq!(iter.nth_back(0), None);
    assert_eq!(iter.last(), None);
}
