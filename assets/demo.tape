# Demo recording for the README. Render with:  vhs assets/demo.tape
#
# The toggle below injects real clicks for about two seconds at the default
# 20 CPS. Park the mouse cursor over the terminal running vhs, or over an
# empty desktop area, before rendering.

Output assets/demo.gif

Set Theme "Catppuccin Mocha"
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set FontSize 15
Set Width 680
Set Height 430
Set Padding 20
Set Framerate 30

Hide
Type "./target/release/clickr" Enter
Sleep 1s
Show

Sleep 1500ms
Space
Sleep 2s
Space
Sleep 800ms
Up Sleep 250ms
Up Sleep 250ms
Up Sleep 250ms
Up Sleep 800ms
Type "m" Sleep 900ms
Type "j" Sleep 900ms
Type "h" Sleep 900ms
Sleep 1200ms
Type "q"
