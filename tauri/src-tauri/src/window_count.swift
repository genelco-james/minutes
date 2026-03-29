/// Counts on-screen windows for specified process names using CoreGraphics.
/// Does NOT require Automation permission (unlike System Events AppleScript).
/// Usage: window_count "Microsoft Teams" "MSTeams"
/// Output: number of layer-0 windows owned by any of the named processes.

import CoreGraphics
import Foundation

let targetNames = Set(CommandLine.arguments.dropFirst().map { $0 })

guard !targetNames.isEmpty else {
    print("0")
    exit(0)
}

let windowList = CGWindowListCopyWindowInfo(
    [.optionOnScreenOnly, .excludeDesktopElements],
    kCGNullWindowID
) as? [[String: Any]] ?? []

var count = 0
for win in windowList {
    if let owner = win[kCGWindowOwnerName as String] as? String {
        if targetNames.contains(owner) {
            let layer = win[kCGWindowLayer as String] as? Int ?? -1
            if layer == 0 {
                count += 1
            }
        }
    }
}

print(count)
