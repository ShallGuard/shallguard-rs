Output semantic-review.gif

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

Type "# A colleague adds a Burst scheduling mode — review time"
Enter
Type "# Step 1/4: inspect the change"
Enter
Type "git log --oneline master..HEAD"
Enter
Type "git --no-pager diff master --color -- src/lib.rs"
Enter
Sleep 4.5s

Type "clear"
Enter
Type "# Step 2/4: tests pass, and the deterministic gate is green"
Enter
Type "cargo test -q"
Enter
Sleep 2.5s
Type "cargo shallguard check"
Enter
Sleep 4s

Type "clear"
Enter
Type "# Step 3/4: but does the change still honor the SHALL contract?"
Enter
Type "# impact -> llvm-cov coverage -> bounded capsule -> local agent verdict"
Enter
Type "cargo shallguard review"
Enter
Sleep 7s
Hide
Sleep 100s
Type "clear"
Enter
Show
Type "# ...the verdict lands half a minute later:"
Enter
Type "cargo shallguard review show"
Enter
Sleep 4.5s

Type "clear"
Enter
Type "# Step 4/4: the stored review — counterexample, citations, suggested fix"
Enter
Type "cargo shallguard review show REQ-HRS-002"
Enter
Sleep 10s
Type "# The agent caught what every green gate missed."
Enter
Sleep 3s
