// Posts synthetic mouse, wheel and keyboard events at a running Roves window on a macOS CI runner
// (GitHub's macOS runners have a real logged-in GUI session, unlike Linux's Xvfb). The macOS
// counterpart of support/xvfb_window_cycle_smoke.sh's xdotool calls; test-page's InputPanel logs
// a "[roves-input]" line for each event that actually reaches the page.
//
// Usage: swift support/macos_input_smoke.swift <pid>
// Posting HID events needs the Accessibility/PostEvent permission; the preflight result is
// printed so a missing permission is distinguishable from an input-delivery bug.
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else {
    print("[roves-macos-input] usage: macos_input_smoke.swift <pid>")
    exit(2)
}

print("[roves-macos-input] post-event access preflight=\(CGPreflightPostEventAccess())")

func findWindowBounds() -> CGRect? {
    guard let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
        return nil
    }
    for window in windows {
        guard (window[kCGWindowOwnerPID as String] as? Int32) == pid,
              (window[kCGWindowLayer as String] as? Int) == 0,
              let boundsDict = window[kCGWindowBounds as String] as? NSDictionary,
              let bounds = CGRect(dictionaryRepresentation: boundsDict as CFDictionary),
              bounds.width > 100, bounds.height > 100
        else { continue }
        return bounds
    }
    return nil
}

var bounds: CGRect?
for _ in 0..<50 {
    bounds = findWindowBounds()
    if bounds != nil { break }
    usleep(200_000)
}
guard let bounds else {
    print("[roves-macos-input] no on-screen window for pid \(pid)")
    exit(1)
}
print("[roves-macos-input] window bounds=\(bounds)")

func post(_ event: CGEvent?) {
    event?.post(tap: .cghidEventTap)
    usleep(80_000)
}

// Top-left padding of the page: clicking there focuses the window without hitting a button.
let point = CGPoint(x: bounds.minX + 40, y: bounds.minY + 60)
post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left))
post(CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: point, mouseButton: .left))
post(CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: point, mouseButton: .left))
usleep(300_000)
post(CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: -3, wheel2: 0, wheel3: 0))
// Virtual key 0 is the "A" key on ANSI layouts.
post(CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true))
post(CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false))
print("[roves-macos-input] posted mouse click, wheel and key events")
