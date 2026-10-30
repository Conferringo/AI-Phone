#![cfg(target_os = "linux")]

use glib::variant::ToVariant;

#[test]
fn optimized_string_iteration_preserves_ffi_output() {
    let values = ["", "手机", "Linux", "🪴", "last"];
    let variant = values.to_variant();
    let mut iter = variant.array_iter_str().unwrap();
    assert_eq!(iter.next(), Some(""));
    assert_eq!(iter.next_back(), Some("last"));
    assert_eq!(iter.nth(1), Some("Linux"));
    assert_eq!(iter.next_back(), Some("🪴"));
    assert_eq!(iter.next(), None);
    assert_eq!(
        variant.array_iter_str().unwrap().collect::<Vec<_>>(),
        values
    );
}

#[test]
fn empty_and_overflowing_iteration_remain_empty() {
    let empty: [&str; 0] = [];
    assert_eq!(empty.to_variant().array_iter_str().unwrap().next(), None);
    let variant = ["first", "last"].to_variant();
    assert_eq!(variant.array_iter_str().unwrap().nth(usize::MAX), None);
    assert_eq!(variant.array_iter_str().unwrap().nth_back(usize::MAX), None);
}
