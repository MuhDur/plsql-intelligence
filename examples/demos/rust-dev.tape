# VHS tape — Rust developer persona demo (PLSQL-LAB-009)
Output examples/demos/recordings/rust-dev.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

Type "# plsql-intelligence — rust-dev persona"
Enter
Sleep 600ms
Type "# canonical type mapping"
Enter
Sleep 600ms
Type "head -45 docs/components/bindings.md | tail -25"
Enter
Sleep 5s

Type "# bindgen tests — IN/OUT modes + Defaulted<T> + diagnostics"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-bindgen --lib 2>&1 | grep 'test result' | tail -3"
Enter
Sleep 4s

Type "# every BG_UNSUPPORTED_* code has a manual-override remediation"
Enter
Sleep 600ms
Type "grep -E 'BG_UNSUPPORTED_' crates/plsql-bindgen/src/lib.rs | head -8"
Enter
Sleep 3s

Type "# done"
Enter
Sleep 2s
