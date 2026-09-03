Output migration-bootstrap.gif

Set Shell "fish"
Set FontSize 15
Set Width 1000
Set Height 650
Set Padding 12
Set TypingSpeed 25ms
Set Theme "Catppuccin Mocha"

Hide
Type "set fish_greeting; set -g fish_color_comment 9399b2; set -g fish_color_command 89b4fa; set -g fish_color_param cdd6f4; set -g fish_color_quote a6e3a1; set -g fish_color_option f9e2af; set -g fish_color_end fab387; set -g fish_color_error f38ba8; clear"
Enter
Show

Type "# Migrating a legacy codebase — phase 1: recover the spec"
Enter
Type "# Step 1/4: four years of code, one test, no written requirements"
Enter
Type "bat -r 8:22 src/lib.rs"
Enter
Sleep 5s

Type "clear"
Enter
Type "# Step 2/4: an agent drafts SHALL contracts from the code and history"
Enter
Type "# — a human reviews every line; evidence starts honest: [pending] ⏳"
Enter
Type "bat docs/REQUIREMENTS.md"
Enter
Sleep 6s

Type "clear"
Enter
Type "# Step 3/4: record today's debt once — the baseline is a ratchet"
Enter
Type "cargo shallguard baseline init"
Enter
Sleep 3.5s

Type "# Step 4/4: the gate passes now and guards every change from day one"
Enter
Type "cargo shallguard check"
Enter
Sleep 6.5s
Type "# 3 known gaps, all tracked. From here the debt can only shrink."
Enter
Sleep 3s
