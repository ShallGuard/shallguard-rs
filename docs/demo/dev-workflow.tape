Output dev-workflow.gif

Set Shell "fish"
Set FontSize 15
Set Width 1000
Set Height 560
Set Padding 12
Set TypingSpeed 25ms
Set Theme "Catppuccin Mocha"

Hide
Type "set fish_greeting; set -g fish_color_comment 9399b2; set -g fish_color_command 89b4fa; set -g fish_color_param cdd6f4; set -g fish_color_quote a6e3a1; set -g fish_color_option f9e2af; set -g fish_color_end fab387; set -g fish_color_error f38ba8; clear"
Enter
Show

Type "# ShallGuard — the SHALL statements guard the code"
Enter
Sleep 1.5s
Type "# Step 1/4: the contract — a numbered SHALL requirement in Markdown"
Enter
Type "bat docs/REQUIREMENTS.md"
Enter
Sleep 5s

Type "clear"
Enter
Type "# Step 2/4: code and test are anchored to the requirement ID"
Enter
Type "bat -r 8:12 -r 32:36 src/lib.rs"
Enter
Sleep 5s

Type "clear"
Enter
Type "# Step 3/4: the gate — cross-check documents against code and tests"
Enter
Type "cargo shallguard check"
Enter
Sleep 5s

Type "clear"
Enter
Type "# Step 4/4: months later, someone deletes that 'redundant' test..."
Enter
Hide
Type `sed -i '/#\[shallguard::verifies("REQ-HRS-001")\]/,/^    }$/d' src/lib.rs`
Enter
Sleep 1s
Show
Type "git --no-pager diff --color"
Enter
Sleep 3.5s
Type "cargo shallguard check"
Enter
Sleep 6s
Type "# Gate closed — CI fails before the evidence is lost."
Enter
Sleep 3s
