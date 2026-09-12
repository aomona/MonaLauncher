// Read only: no activation, focus changes, mouse events, or keyboard events.
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int(CommandLine.arguments[1]), pid > 0 else {
    fputs("usage: minecraft_window_state.swift PID\n", stderr)
    exit(2)
}
let raw = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as? [[String: Any]] ?? []
let windows = raw.filter {
    $0[kCGWindowOwnerPID as String] as? Int == pid &&
    ($0[kCGWindowName as String] as? String ?? "").hasPrefix("Minecraft") &&
    $0[kCGWindowLayer as String] as? Int == 0
}
let states: [[String: Any]] = windows.map { window in
    [
        "windowId": window[kCGWindowNumber as String] as? Int ?? 0,
        "title": window[kCGWindowName as String] as? String ?? "",
        "onScreen": window[kCGWindowIsOnscreen as String] as? Bool ?? false,
        "alpha": window[kCGWindowAlpha as String] as? Double ?? 0,
        "bounds": window[kCGWindowBounds as String] as? [String: Any] ?? [:]
    ]
}
let visible = states.contains { ($0["onScreen"] as? Bool ?? false) && ($0["alpha"] as? Double ?? 0) > 0 }
let report: [String: Any] = ["pid": pid, "visible": visible, "windows": states]
let data = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
exit(visible ? 0 : 1)
