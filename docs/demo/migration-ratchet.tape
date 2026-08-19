Output migration-ratchet.gif

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

Type "# Migrating a legacy codebase — phase 2: the evidence ratchet"
Enter
Type "# Step 1/4: pick one pending contract"
Enter
Type "bat -r 16:21 docs/REQUIREMENTS.md"
Enter
Sleep 4.5s

Type "clear"
Enter
Type "# Step 2/4: the agent anchors the code and writes a real test — you review"
Enter
Hide
Type "python3 ../apply_ratchet.py"
Enter
Sleep 1s
Show
Type "git --no-pager diff --color"
Enter
Sleep 6s

Type "clear"
Enter
Type "# Step 3/4: prove it, then shrink the recorded debt"
Enter
Type "cargo test -q"
Enter
Sleep 3s
Type "cargo shallguard baseline prune"
Enter
Sleep 3s

Type "# Step 4/4: the ratchet tightens — per-area coverage shows the progress"
Enter
Type "cargo shallguard check"
Enter
Sleep 6.5s
Type "# Repeat per contract, harden each clean area, then develop requirement-first."
Enter
Sleep 3s
