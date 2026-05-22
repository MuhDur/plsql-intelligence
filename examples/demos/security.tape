# VHS tape — security-auditor persona demo (PLSQL-LAB-009)
Output examples/demos/recordings/security.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

Type "# plsql-intelligence — security-auditor persona"
Enter
Sleep 600ms
Type "# privilege doctor: cross-schema-writes + ambiguity"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-privileges --lib doctor 2>&1 | tail -8"
Enter
Sleep 4s

Type "# K18 sanitizer: prompt-injection markers"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-mcp sanitize 2>&1 | tail -3"
Enter
Sleep 3s

Type "# is_read_only_sql: FOR UPDATE + multi-statement reject"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-mcp read_only_predicate 2>&1 | tail -3"
Enter
Sleep 3s

Type "# done"
Enter
Sleep 2s
