// Phase 0 Spike: Permission lifecycle validation
// Run in Xcode on macOS 15+ to test mic + screen capture behavior.

import SwiftUI

@main
struct PermissionSpikeApp: App {
    var body: some Scene {
        WindowGroup {
            PermissionCheckView()
        }
    }
}
