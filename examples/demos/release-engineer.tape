# VHS tape — release-engineer persona demo (PLSQL-LAB-009)
#
# Records examples/demos/release-engineer.md as an animated GIF.
# Run via `make demo-record` (requires `charmbracelet/vhs` installed:
# https://github.com/charmbracelet/vhs).

Output examples/demos/recordings/release-engineer.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

Type "# plsql-intelligence — release-engineer persona"
Enter
Sleep 800ms
Type "# step 1: predict invalidations from the hero diff"
Enter
Sleep 600ms
Type "cargo test --quiet -p plsql-lineage parse_change_file 2>&1 | tail -3"
Enter
Sleep 4s

Type "# step 2: read the policy file"
Enter
Sleep 600ms
Type "cat examples/demos/release-engineer.md | head -40"
Enter
Sleep 3s

Type "# step 3: run the static-only gate"
Enter
Sleep 600ms
Type "make demo-no-db-verify 2>&1 | tail -5"
Enter
Sleep 4s

Type "# done — no Oracle connection required"
Enter
Sleep 2s
