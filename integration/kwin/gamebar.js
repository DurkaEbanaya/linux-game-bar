// Native KWin 6 adapter: remember the game BEFORE our overlay receives keyboard focus.
function publish(window) {
    if (!window || String(window.resourceClass) === "linux-game-bar") return;
    callDBus("io.github.DurkaEbanaya.GameBar", "/io/github/DurkaEbanaya/GameBar",
        "io.github.DurkaEbanaya.GameBar", "ActiveWindow", JSON.stringify({
            pid: window.pid || null, title: window.caption || "",
            output: window.output ? window.output.name : null, window_id: String(window.internalId)
        }));
}
workspace.windowActivated.connect(publish);
publish(workspace.activeWindow);
registerShortcut("LinuxGameBarToggle", "Linux Game Bar: toggle overlay", "Meta+G", function () {
    publish(workspace.activeWindow);
    callDBus("io.github.DurkaEbanaya.GameBar", "/io/github/DurkaEbanaya/GameBar",
        "io.github.DurkaEbanaya.GameBar", "Toggle");
});
registerShortcut("LinuxGameBarHudToggle", "Linux Game Bar: show/hide HUD", "Meta+Shift+G", function () {
    publish(workspace.activeWindow);
    callDBus("io.github.DurkaEbanaya.GameBar", "/io/github/DurkaEbanaya/GameBar",
        "io.github.DurkaEbanaya.GameBar", "ToggleHud");
});
