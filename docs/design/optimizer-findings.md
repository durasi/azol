# Costs the first run of the gate found that no planned pass covers

From the baseline of the 28-program suite (default mode, x86-64, 12 November
2026: geometric mean 1.61, 22 programs above 1.10). These must be added to
the plan in [optimizer.md](optimizer.md).

1. **Owned substrings.** `slice` and `split` allocate where Rust borrows a
   `&str`. Seen in `tokenizer`, `words` (2.07) and `wordfreq` (1.93). Needs
   string views that the checker proves do not outlive their source, or
   stack promotion of slices that do not escape.
2. **Membership in a constant set of strings.** `keywords.contains(word)` is
   42% of `tokenizer`: up to 18 runtime compares per word where Rust's
   `match` is a switch on length. Needs `match` on strings compiled to a
   length switch, and `contains` on a constant list lowered the same way.
3. **No unsigned shift.** `>>` followed by a mask must be recognised as a
   logical shift, or the language needs one (the `u64` type of legacy.md
   brings it).
4. **Swapping two lists copies.** Forces a one-list table in `dp_lcs`
   (2.21). Needs a `swap` that exchanges the buffers, and the optimiser
   treating it as a move.
5. **Library gaps that cost speed:** the prelude merge sort (`sort_lib`
   8.22), no `popcount`, no queue, no heap, no boxed node.
