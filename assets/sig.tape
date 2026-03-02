Output assets/sig.gif

Set Shell "bash"
Set Theme "Dracula"
Set PlaybackSpeed 0.5
Set FontSize 28
Set Width 1800
Set Height 1200

Type@20ms "kubectl -n kube-system logs etcd-kind-control-plane | sig --render-interval 50" Sleep 300ms
Enter Sleep 2s
Type@100ms "info" Sleep 4s
