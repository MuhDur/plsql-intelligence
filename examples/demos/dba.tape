# VHS tape — DBA persona demo (PLSQL-LAB-009)
Output examples/demos/recordings/dba.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

Type "# plsql-intelligence — DBA persona"
Enter
Sleep 600ms
Type "# step 1: catalog doctor surfaces"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-catalog --lib doctor 2>&1 | tail -5"
Enter
Sleep 4s

Type "# step 2: VPD policies"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-catalog --lib vpd_policy 2>&1 | tail -3"
Enter
Sleep 3s

Type "# step 3: editioning views"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-catalog --lib edition 2>&1 | tail -3"
Enter
Sleep 3s

Type "# done"
Enter
Sleep 2s
