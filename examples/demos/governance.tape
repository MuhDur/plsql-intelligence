# VHS tape — governance / compliance persona demo (PLSQL-LAB-009)
Output examples/demos/recordings/governance.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

Type "# plsql-intelligence — governance persona"
Enter
Sleep 600ms
Type "# every layer ships a doctor surface"
Enter
Sleep 600ms
Type "cargo test --quiet --workspace --lib doctor 2>&1 | grep 'test result' | head -6"
Enter
Sleep 6s

Type "# R13: typed UnknownReason discipline — 12 variants"
Enter
Sleep 600ms
Type "grep -E '^\\s+[A-Z][a-zA-Z]+,$' crates/plsql-core/src/lib.rs | head -12"
Enter
Sleep 3s

Type "# done"
Enter
Sleep 2s
