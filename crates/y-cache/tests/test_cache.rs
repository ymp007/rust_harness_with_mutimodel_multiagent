use y_cache::{RadixPrefixCache, RoPEPositionShifter};

#[test]
fn test_radix_prefix_cache_matching() {
    let cache = RadixPrefixCache::new();

    let seq1 = [100, 200, 300, 400, 500];
    cache.insert_sequence(&seq1);

    // Exact match
    let match_len = cache.find_longest_prefix(&seq1);
    assert_eq!(match_len, 5);

    // Prefix match
    let seq2 = [100, 200, 300, 999];
    let prefix_match = cache.find_longest_prefix(&seq2);
    assert_eq!(prefix_match, 3);

    // No match
    let seq3 = [999, 888];
    assert_eq!(cache.find_longest_prefix(&seq3), 0);
}

#[test]
fn test_rope_frequency_calculation() {
    let shifter = RoPEPositionShifter::with_default_llama();
    let freqs = shifter.compute_frequency_multiplier(64, 128);
    assert_eq!(freqs.len(), 64);
    assert!(freqs[0] > 0.0);
}
