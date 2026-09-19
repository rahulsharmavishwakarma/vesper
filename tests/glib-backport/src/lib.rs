#[cfg(test)]
mod tests {
    use glib::variant::ToVariant;

    #[test]
    fn optimized_string_iterator_covers_both_directions_and_skips() {
        let values = ["alpha", "βeta", "", "नमस्ते", "last"];
        let variant = values.to_variant();
        assert_eq!(
            variant.array_iter_str().unwrap().collect::<Vec<_>>(),
            values
        );
        assert_eq!(variant.array_iter_str().unwrap().last(), Some("last"));
        assert_eq!(variant.array_iter_str().unwrap().nth(3), Some("नमस्ते"));
        assert_eq!(variant.array_iter_str().unwrap().nth_back(3), Some("βeta"));
        let mut iter = variant.array_iter_str().unwrap();
        assert_eq!(iter.next(), Some("alpha"));
        assert_eq!(iter.next_back(), Some("last"));
        assert_eq!(iter.collect::<Vec<_>>(), ["βeta", "", "नमस्ते"]);
    }

    #[test]
    fn empty_and_exhausted_iterators_stay_empty() {
        let empty: [&str; 0] = [];
        let variant = empty.to_variant();
        assert_eq!(variant.array_iter_str().unwrap().next(), None);
        assert_eq!(variant.array_iter_str().unwrap().next_back(), None);
        assert_eq!(variant.array_iter_str().unwrap().nth(usize::MAX), None);
        let variant = ["one"].to_variant();
        let mut iter = variant.array_iter_str().unwrap();
        assert_eq!(iter.next_back(), Some("one"));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
    }
}
