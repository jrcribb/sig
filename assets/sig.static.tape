Output assets/sig.static.gif

Set Shell "bash"
Set Theme "Dracula"
Set FontSize 28
Set Width 1800
Set Height 1200

Type@50ms "cat README.md | sig" Sleep 1s
Enter Sleep 3s
Type@100ms "sig" Sleep 3s
Type@100ms " | Interactive" Sleep 3s
