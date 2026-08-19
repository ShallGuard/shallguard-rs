Output review-workflow.gif

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

Type "# Reviewing a colleague's branch with ShallGuard"
Enter
Type "# Step 1/4: inspect the branch"
Enter
Type "git log --oneline master..HEAD"
Enter
Type "git --no-pager diff master --stat"
Enter
Sleep 3s

Type "clear"
Enter
Type "# Step 2/4: which requirement contracts does this diff touch?"
Enter
Type "cargo shallguard impact --base master --json impact.json --markdown impact.md"
Enter
Sleep 2s
Type "bat impact.md"
Enter
Sleep 5s

Type "clear"
Enter
Type "# Step 3/4: the touched contract, and the test that proves it"
Enter
Type "bat -r 13:15 docs/REQUIREMENTS.md"
Enter
Sleep 4.5s
Type "# Run exactly that test"
Enter
Type "cargo test resolve_applies_floor_in_every_mode"
Enter
Sleep 4s

Type "clear"
Enter
Type "# Step 4/4: the traceability gate still holds on the branch"
Enter
Type "cargo shallguard check"
Enter
Sleep 5s
Type "# Spec, code, and evidence reviewed as one unit."
Enter
Sleep 3s
